#![forbid(unsafe_code)]
#![cfg(feature = "db-integration-tests")]

use fanwaave_interfaces::contact_read::{
    ContactReadGrantClaims, CONTACT_READ_AUDIENCE, CONTACT_READ_ISSUER, CONTACT_READ_SCOPE,
};
use fanwaave_lib_core::{
    config::CoreConfig,
    contact::{ContactChannel, ContactJobStatus, ContactProvider},
    flavor::DatabaseFlavor,
    ReadContext, ReadError, ReadVerifierKey,
};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use sqlx::PgPool;
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

const EXPECTED_ID: &str = "00000000-0000-4000-8000-000000000001";
const MISSING_ID: &str = "00000000-0000-4000-8000-000000000002";
const TEST_SECRET: &str = "test-only-fanwaave-read-secret-32-chars";
const PREVIOUS_SECRET: &str = "test-only-fanwaave-prev-secret-32-chars";

fn keys() -> [ReadVerifierKey<'static>; 2] {
    [
        ReadVerifierKey {
            kid: "current",
            secret: TEST_SECRET.as_bytes(),
        },
        ReadVerifierKey {
            kid: "previous",
            secret: PREVIOUS_SECRET.as_bytes(),
        },
    ]
}

fn read_config() -> CoreConfig {
    let dsn = std::env::var("FANWAAVE_READ_TEST_DSN")
        .expect("dedicated PostgreSQL source CI must supply a SELECT-only role DSN");
    CoreConfig::from_values(dsn, None).expect("test DSN parses")
}

fn expected_uuid() -> Uuid {
    Uuid::parse_str(EXPECTED_ID).expect("fixed UUID is well-formed")
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("CI clock has a positive Unix time")
        .as_secs() as i64
}

/// Test fixture issuer only. Production has NO token issuance API here.
fn signed_grant(
    resource: Uuid,
    kid: &str,
    signing_key: &str,
    edit: impl FnOnce(&mut ContactReadGrantClaims),
) -> String {
    let instant = now();
    let mut claims = ContactReadGrantClaims {
        iss: CONTACT_READ_ISSUER.into(),
        aud: CONTACT_READ_AUDIENCE.into(),
        sub: "test-only-service-principal".into(),
        scope: CONTACT_READ_SCOPE.into(),
        job_id: resource.to_string(),
        iat: instant - 1,
        nbf: instant - 1,
        exp: instant + 120,
        jti: "00000000-0000-4000-8000-000000000003".into(),
    };
    edit(&mut claims);
    let mut header = Header::new(Algorithm::HS256);
    header.kid = Some(kid.to_owned());
    let signed = encode(
        &header,
        &claims,
        &EncodingKey::from_secret(signing_key.as_bytes()),
    )
    .expect("test-only issuer can create a fixture token");
    format!("Bearer {signed}")
}

#[tokio::test]
async fn reads_real_contact_job_and_unknown_id_is_none() {
    let config = read_config();
    let reader = ReadContext::connect(&config, &keys())
        .await
        .expect("live SELECT-only reader opens");
    let auth = signed_grant(expected_uuid(), "current", TEST_SECRET, |_| {});
    let row = reader
        .get_contact_job(expected_uuid(), &auth)
        .await
        .expect("query succeeds")
        .expect("fixture row exists");
    assert_eq!(row.job_id, EXPECTED_ID);
    assert_eq!(row.idempotency_key, "fixture-contact-job-0001");
    assert_eq!(row.channel, ContactChannel::Email);
    assert_eq!(row.provider, ContactProvider::Sendgrid);
    assert_eq!(row.status, ContactJobStatus::Queued);
    assert_eq!(row.attempt_count, 0);
    assert_eq!(row.max_attempts, 8);
    assert!(row.provider_message_id.is_none());

    let unknown = Uuid::parse_str(MISSING_ID).expect("well-formed UUID");
    let auth_other = signed_grant(unknown, "previous", PREVIOUS_SECRET, |_| {});
    assert!(reader
        .get_contact_job(unknown, &auth_other)
        .await
        .expect("unknown ID is a legitimate read")
        .is_none());

    let debug = format!("{reader:?}");
    assert!(!debug.contains(TEST_SECRET));
    assert!(!debug.contains(PREVIOUS_SECRET));
    assert!(!debug.contains(&config.database_url));
}

#[tokio::test]
async fn denies_unscoped_wrong_job_expired_tampered_and_unknown_keys() {
    let config = read_config();
    let reader = ReadContext::connect(&config, &keys())
        .await
        .expect("reader opens");

    let valid = signed_grant(expected_uuid(), "current", TEST_SECRET, |_| {});
    let other_job = signed_grant(
        Uuid::parse_str(MISSING_ID).expect("fixture UUID"),
        "current",
        TEST_SECRET,
        |_| {},
    );
    let wrong_scope = signed_grant(expected_uuid(), "current", TEST_SECRET, |c| {
        c.scope = "contact_job.write".into();
    });
    let expired = signed_grant(expected_uuid(), "current", TEST_SECRET, |c| {
        c.iat = now() - 500;
        c.nbf = now() - 500;
        c.exp = now() - 100;
    });
    let long_lived = signed_grant(expected_uuid(), "current", TEST_SECRET, |c| {
        c.exp = now() + 3_600;
    });
    let future = signed_grant(expected_uuid(), "current", TEST_SECRET, |c| {
        c.nbf = now() + 100;
        c.exp = now() + 120;
    });
    let unknown_kid = signed_grant(expected_uuid(), "unknown", TEST_SECRET, |_| {});
    let tampered = format!("{valid}changed");

    for header in [
        "",
        "Bearer ",
        "Bearer test-only-fanwaave-read-secret-32-chars",
        valid.strip_prefix("Bearer ").expect("prefix"),
        &other_job,
        &wrong_scope,
        &expired,
        &long_lived,
        &future,
        &unknown_kid,
        &tampered,
    ] {
        assert_eq!(
            reader.get_contact_job(expected_uuid(), header).await.err(),
            Some(ReadError::Unauthorized),
            "invalid or unscoped bearer must never be accepted"
        );
    }
}

#[tokio::test]
async fn key_rotation_accepts_overlap_but_refuses_retired_key() {
    let config = read_config();
    let old = signed_grant(expected_uuid(), "previous", PREVIOUS_SECRET, |_| {});
    let reader = ReadContext::connect(&config, &keys())
        .await
        .expect("reader with overlapping known keys opens");
    assert!(reader.get_contact_job(expected_uuid(), &old).await.is_ok());

    let current = [ReadVerifierKey {
        kid: "current",
        secret: TEST_SECRET.as_bytes(),
    }];
    let rotated = ReadContext::connect(&config, &current)
        .await
        .expect("reader with retired previous key opens");
    assert_eq!(
        rotated.get_contact_job(expected_uuid(), &old).await.err(),
        Some(ReadError::Unauthorized)
    );
}

#[tokio::test]
async fn database_role_cannot_mutate_even_using_separate_connection() {
    let config = read_config();
    let pool = PgPool::connect(&config.database_url)
        .await
        .expect("SELECT-only role can connect");

    // Exercise real database grants independently of the reader's session
    // read-only default; the role itself cannot write.
    let update = sqlx::query("UPDATE public.contact_jobs SET status = 'sent' WHERE id = $1")
        .bind(expected_uuid())
        .execute(&pool)
        .await;
    assert!(update.is_err(), "SELECT-only role must not UPDATE");

    let insert = sqlx::query(
        "INSERT INTO public.contact_jobs (id, idempotency_key, channel, provider, payload) \
         VALUES ($1, 'not-allowed-fixture', 'email', 'sendgrid', '{}'::jsonb)",
    )
    .bind(Uuid::parse_str(MISSING_ID).expect("fixed UUID"))
    .execute(&pool)
    .await;
    assert!(insert.is_err(), "SELECT-only role must not INSERT");
}

#[tokio::test]
async fn refuses_elevated_roles_even_if_contact_table_itself_is_readonly() {
    let config = read_config();
    let writable_role = std::env::var("FANWAAVE_READ_EXTRA_PRIVILEGE_DSN")
        .expect("CI must supply a role with write privileges on an unrelated table");
    let extra = CoreConfig::from_values(writable_role, None).expect("DSN parses");
    assert!(matches!(
        ReadContext::connect(&extra, &keys()).await,
        Err(ReadError::ExcessivePrivileges)
    ));
    let admin_dsn =
        std::env::var("FANWAAVE_READ_ADMIN_TEST_DSN").expect("CI must supply a database owner DSN");
    let admin = CoreConfig::from_values(admin_dsn, None).expect("admin DSN parses");
    assert!(matches!(
        ReadContext::connect(&admin, &keys()).await,
        Err(ReadError::ExcessivePrivileges)
    ));
    assert!(ReadContext::connect(&config, &keys()).await.is_ok());
}

#[tokio::test]
async fn refuses_invalid_keyring_and_config_before_database() {
    let config = read_config();
    let mut writable = config.clone();
    writable.read_only = false;
    assert!(matches!(
        ReadContext::connect(&writable, &keys()).await,
        Err(ReadError::NotReadOnly)
    ));
    let short = [ReadVerifierKey {
        kid: "short",
        secret: b"short",
    }];
    assert!(matches!(
        ReadContext::connect(&config, &short).await,
        Err(ReadError::InvalidReadCredential)
    ));
    assert!(matches!(
        ReadContext::connect(&config, &[]).await,
        Err(ReadError::InvalidReadCredential)
    ));
    let duplicate = [
        ReadVerifierKey {
            kid: "same",
            secret: TEST_SECRET.as_bytes(),
        },
        ReadVerifierKey {
            kid: "same",
            secret: PREVIOUS_SECRET.as_bytes(),
        },
    ];
    assert!(matches!(
        ReadContext::connect(&config, &duplicate).await,
        Err(ReadError::InvalidReadCredential)
    ));
    let mut cockroach = config.clone();
    cockroach.flavor = DatabaseFlavor::CockroachDb;
    assert!(matches!(
        ReadContext::connect(&cockroach, &keys()).await,
        Err(ReadError::UnsupportedFlavor)
    ));
}
