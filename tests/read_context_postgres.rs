#![forbid(unsafe_code)]
#![cfg(feature = "postgres-read")]

use std::sync::Arc;

use fanwaave_lib_core::{
    config::CoreConfig,
    contact::{ContactChannel, ContactJobStatus},
    read_context::{ReadAuthorizer, ReadContext, ReadError, ReadOperation},
};
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

// Test-only authorization. Production must supply a reviewed, real identity
// and per-record scope policy; this is not a production credential scheme.
struct ScopedTestAuthorizer {
    allowed_id: Uuid,
}

impl ReadAuthorizer for ScopedTestAuthorizer {
    fn authorize(&self, authorization: &str, operation: ReadOperation) -> Result<(), ReadError> {
        match operation {
            ReadOperation::ContactJob { id }
                if id == self.allowed_id && authorization == "Bearer ephemeral-ci-read" =>
            {
                Ok(())
            }
            _ => Err(ReadError::Unauthorized),
        }
    }
}

#[tokio::test]
async fn actual_postgres_enforces_role_authorization_and_read_only_queries() {
    // The CI service is required; absence is a failure, never a silent skip.
    let admin_url = std::env::var("FANWAAVE_TEST_DATABASE_URL")
        .expect("real PostgreSQL service URL required for integration testing");
    let admin = PgPoolOptions::new()
        .max_connections(2)
        .connect(&admin_url)
        .await
        .expect("connect to ephemeral PostgreSQL service");

    sqlx::raw_sql(include_str!("../migrations/0002_contact_delivery.sql"))
        .execute(&admin)
        .await
        .expect("apply canonical contact schema");
    sqlx::query(
        r#"
        DO $$
        BEGIN
            IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'fanwaave_read_contract_ci') THEN
                CREATE ROLE fanwaave_read_contract_ci LOGIN PASSWORD 'ephemeral-only-ci';
            END IF;
        END
        $$;
        "#,
    )
    .execute(&admin)
    .await
    .expect("create disposable read-only test role");
    sqlx::query("GRANT USAGE ON SCHEMA public TO fanwaave_read_contract_ci")
        .execute(&admin)
        .await
        .expect("grant schema access");
    sqlx::query("GRANT SELECT ON TABLE public.contact_jobs TO fanwaave_read_contract_ci")
        .execute(&admin)
        .await
        .expect("grant table read permission");

    let job_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO public.contact_jobs
            (idempotency_key, channel, provider, payload, status)
        VALUES
            ('fanwaave-read-contract-ci', 'email', 'sendgrid', '{}'::jsonb, 'queued')
        ON CONFLICT (idempotency_key) DO UPDATE SET status = 'queued'
        RETURNING id
        "#,
    )
    .fetch_one(&admin)
    .await
    .expect("insert real contact-job fixture");

    let mut reader_url = url::Url::parse(&admin_url).expect("valid admin URL");
    reader_url
        .set_username("fanwaave_read_contract_ci")
        .expect("valid read-only role name");
    reader_url
        .set_password(Some("ephemeral-only-ci"))
        .expect("valid ephemeral CI password");
    let reader_config = CoreConfig::from_values(reader_url.to_string(), None)
        .expect("valid PostgreSQL read-only config");
    assert!(reader_config.read_only);

    let policy = Arc::new(ScopedTestAuthorizer { allowed_id: job_id });
    let context = ReadContext::connect(&reader_config, policy.clone())
        .await
        .expect("read-only database role admitted");

    assert!(matches!(
        context.get_contact_job(job_id, "Bearer wrong").await,
        Err(ReadError::Unauthorized)
    ));
    assert!(matches!(
        context
            .get_contact_job(Uuid::nil(), "Bearer ephemeral-ci-read")
            .await,
        Err(ReadError::Unauthorized)
    ));

    let loaded = context
        .get_contact_job(job_id, "Bearer ephemeral-ci-read")
        .await
        .expect("query must succeed")
        .expect("persisted job must exist");
    assert_eq!(loaded.job_id, job_id.to_string());
    assert_eq!(loaded.channel, ContactChannel::Email);
    assert_eq!(loaded.status, ContactJobStatus::Queued);
    assert_eq!(loaded.idempotency_key, "fanwaave-read-contract-ci");

    // Independent database privilege enforcement, outside the Rust context.
    let reader_pool = PgPool::connect(reader_url.as_str())
        .await
        .expect("connect with SELECT-only role");
    let attempted_write = sqlx::query("UPDATE public.contact_jobs SET status='sent' WHERE id=$1")
        .bind(job_id)
        .execute(&reader_pool)
        .await;
    assert!(
        attempted_write.is_err(),
        "the reader role must not update contact jobs"
    );

    // A writer role must be rejected even if config.read_only says true.
    let admin_config = CoreConfig::from_values(admin_url, None).expect("valid admin config");
    assert!(matches!(
        ReadContext::connect(&admin_config, policy.clone()).await,
        Err(ReadError::PrivilegedDatabaseRole)
    ));

    // A write-enabled configuration must fail before a connection is opened.
    let writer_config = CoreConfig::from_values(reader_url.to_string(), Some("0"))
        .expect("valid explicit writer config");
    assert!(matches!(
        ReadContext::connect(&writer_config, policy).await,
        Err(ReadError::ReadOnlyRequired)
    ));
}
