#![forbid(unsafe_code)]
#![cfg(feature = "read-only")]

use fanwaave_lib_core::{
    config::CoreConfig,
    contact::{ContactChannel, ContactJobStatus, ContactProvider},
    flavor::DatabaseFlavor,
    ReadContext, ReadError,
};
use sqlx::PgPool;
use uuid::Uuid;

const EXPECTED_ID: &str = "00000000-0000-4000-8000-000000000001";
const MISSING_ID: &str = "00000000-0000-4000-8000-000000000002";
const TEST_SECRET: &str = "test-only-fanwaave-read-secret-32-chars";
const TEST_AUTH: &str = "Bearer test-only-fanwaave-read-secret-32-chars";

fn read_config() -> CoreConfig {
    let dsn = std::env::var("FANWAAVE_READ_TEST_DSN")
        .expect("CI must supply a live SELECT-only PostgreSQL role DSN");
    CoreConfig::from_values(dsn, None).expect("test DSN parses")
}

fn expected_uuid() -> Uuid {
    Uuid::parse_str(EXPECTED_ID).expect("fixed UUID is well-formed")
}

#[tokio::test]
async fn reads_real_contact_job_and_unknown_id_is_none() {
    let config = read_config();
    let reader = ReadContext::connect(&config, TEST_SECRET)
        .await
        .expect("live SELECT-only reader opens");
    let row = reader.get_contact_job(expected_uuid(), TEST_AUTH)
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
    assert!(reader.get_contact_job(
        Uuid::parse_str(MISSING_ID).expect("well-formed UUID"), TEST_AUTH
    ).await.expect("unknown ID is a legitimate read").is_none());

    let debug = format!("{reader:?}");
    assert!(!debug.contains(TEST_SECRET));
    assert!(!debug.contains(&config.database_url));
}

#[tokio::test]
async fn denies_unauthorized_requests_before_a_query() {
    let config = read_config();
    let reader = ReadContext::connect(&config, TEST_SECRET)
        .await
        .expect("reader opens");
    for header in [
        "",
        "Bearer ",
        "Basic test-only-fanwaave-read-secret-32-chars",
        "Bearer test-only-fanwaave-read-secret-32-charX",
        "bearer test-only-fanwaave-read-secret-32-chars",
    ] {
        assert_eq!(
            reader.get_contact_job(expected_uuid(), header).await.err(),
            Some(ReadError::Unauthorized)
        );
    }
}

#[tokio::test]
async fn database_role_cannot_mutate_even_using_separate_connection() {
    let config = read_config();
    let pool = PgPool::connect(&config.database_url)
        .await
        .expect("SELECT-only role can connect");

    // Exercise the real database grants independently of the read-context's
    // session read-only setting; these SQL writes must fail at the DB layer.
    let update = sqlx::query(
        "UPDATE public.contact_jobs SET status = 'sent' WHERE id = $1",
    )
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
async fn refuses_writable_role_and_invalid_reader_configuration() {
    let config = read_config();
    let mut write_config = config.clone();
    write_config.read_only = false;
    assert!(matches!(
        ReadContext::connect(&write_config, TEST_SECRET).await,
        Err(ReadError::NotReadOnly)
    ));
    assert!(matches!(
        ReadContext::connect(&config, "too-short").await,
        Err(ReadError::InvalidReadCredential)
    ));
    let mut cockroach = config.clone();
    cockroach.flavor = DatabaseFlavor::CockroachDb;
    assert!(matches!(
        ReadContext::connect(&cockroach, TEST_SECRET).await,
        Err(ReadError::UnsupportedFlavor)
    ));

    let admin_dsn = std::env::var("FANWAAVE_READ_ADMIN_TEST_DSN")
        .expect("CI must supply test administrator DSN");
    let admin_config = CoreConfig::from_values(admin_dsn, None)
        .expect("admin DSN parses");
    assert!(matches!(
        ReadContext::connect(&admin_config, TEST_SECRET).await,
        Err(ReadError::ExcessivePrivileges)
    ));
}
