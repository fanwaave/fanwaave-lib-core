#![forbid(unsafe_code)]
//! A real opt-in PostgreSQL read context for the canonical contact-delivery
//! schema. There is no arbitrary SQL or write API.
//!
//! A caller supplies an explicit per-operation authorizer. A distinct DB role
//! must independently hold SELECT without any table mutation privileges.

use std::sync::Arc;
use sqlx::{postgres::PgPoolOptions, PgPool};
use uuid::Uuid;

use crate::{
    config::CoreConfig,
    contact::{ContactChannel, ContactJobStatus, ContactJobView, ContactProvider},
};

type ContactJobRow = (
    Uuid,
    String,
    String,
    String,
    String,
    i32,
    i32,
    Option<String>,
    Option<String>,
);

const GET_CONTACT_JOB: &str = r#"
    SELECT id, idempotency_key, channel, provider, status, attempt_count, max_attempts,
           provider_message_id, last_error
    FROM public.contact_jobs
    WHERE id = $1
"#;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadOperation {
    ContactJob { id: Uuid },
}

/// Domain-owned authorization hook. No permissive/default implementation is
/// provided. The decision must cover the specific operation and its record id.
pub trait ReadAuthorizer: Send + Sync {
    fn authorize(
        &self,
        authorization: &str,
        operation: ReadOperation,
    ) -> Result<(), ReadError>;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum ReadError {
    #[error("read-only database configuration is required")]
    ReadOnlyRequired,
    #[error("database role must have SELECT and no mutating table privileges")]
    PrivilegedDatabaseRole,
    #[error("operation is not authorized")]
    Unauthorized,
    #[error("read-only database operation failed")]
    Database,
    #[error("stored contact job violates the canonical contract")]
    InvalidStoredData,
}

/// Real DB-backed reader. Opt in to the postgres-read Cargo feature.
/// Database credentials and raw sqlx diagnostics are never included in errors.
pub struct ReadContext {
    pool: PgPool,
    authorizer: Arc<dyn ReadAuthorizer>,
}

impl ReadContext {
    /// Connect and refuse a writer role or incompatible physical table
    /// projection before allowing any read operation.
    pub async fn connect(
        config: &CoreConfig,
        authorizer: Arc<dyn ReadAuthorizer>,
    ) -> Result<Self, ReadError> {
        if !config.read_only {
            return Err(ReadError::ReadOnlyRequired);
        }
        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.database_url)
            .await
            .map_err(|_| ReadError::Database)?;
        verify_readonly_role(&pool).await?;
        // A zero-row actual statement validates table and named columns.
        sqlx::query(GET_CONTACT_JOB)
            .bind(Uuid::nil())
            .fetch_optional(&pool)
            .await
            .map_err(|_| ReadError::Database)?;
        Ok(Self { pool, authorizer })
    }

    /// Authorize this record before touching the DB and execute a parameterized
    /// SELECT within an explicitly read-only transaction. There is no query
    /// interpolation or method to mutate application records.
    pub async fn get_contact_job(
        &self,
        id: Uuid,
        authorization: &str,
    ) -> Result<Option<ContactJobView>, ReadError> {
        self.authorizer
            .authorize(authorization, ReadOperation::ContactJob { id })?;

        let mut transaction = self.pool.begin().await.map_err(|_| ReadError::Database)?;
        sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *transaction)
            .await
            .map_err(|_| ReadError::Database)?;
        let found = sqlx::query_as::<_, ContactJobRow>(GET_CONTACT_JOB)
            .bind(id)
            .fetch_optional(&mut *transaction)
            .await
            .map_err(|_| ReadError::Database)?;
        transaction.commit().await.map_err(|_| ReadError::Database)?;
        found.map(decode_contact_job).transpose()
    }
}

async fn verify_readonly_role(pool: &PgPool) -> Result<(), ReadError> {
    // Deny a writer role even if our own implementation currently only SELECTs.
    let (select, insert, update, delete, truncate): (bool, bool, bool, bool, bool) =
        sqlx::query_as(
            r#"
            SELECT has_table_privilege(current_user, 'public.contact_jobs', 'SELECT'),
                   has_table_privilege(current_user, 'public.contact_jobs', 'INSERT'),
                   has_table_privilege(current_user, 'public.contact_jobs', 'UPDATE'),
                   has_table_privilege(current_user, 'public.contact_jobs', 'DELETE'),
                   has_table_privilege(current_user, 'public.contact_jobs', 'TRUNCATE')
            "#,
        )
        .fetch_one(pool)
        .await
        .map_err(|_| ReadError::Database)?;
    if !select || insert || update || delete || truncate {
        return Err(ReadError::PrivilegedDatabaseRole);
    }
    Ok(())
}

fn decode_contact_job(row: ContactJobRow) -> Result<ContactJobView, ReadError> {
    let (
        id,
        idempotency_key,
        channel,
        provider,
        status,
        attempt_count,
        max_attempts,
        provider_message_id,
        last_error,
    ) = row;
    let channel = match channel.as_str() {
        "email" => ContactChannel::Email,
        "sms" => ContactChannel::Sms,
        _ => return Err(ReadError::InvalidStoredData),
    };
    let provider = match provider.as_str() {
        "sendgrid" => ContactProvider::Sendgrid,
        "twilio" => ContactProvider::Twilio,
        _ => return Err(ReadError::InvalidStoredData),
    };
    let status = match status.as_str() {
        "queued" => ContactJobStatus::Queued,
        "leased" => ContactJobStatus::Leased,
        "retry" => ContactJobStatus::Retry,
        "sent" => ContactJobStatus::Sent,
        "dead" => ContactJobStatus::Dead,
        "cancelled" => ContactJobStatus::Cancelled,
        _ => return Err(ReadError::InvalidStoredData),
    };
    if attempt_count < 0 || max_attempts < 1 {
        return Err(ReadError::InvalidStoredData);
    }
    Ok(ContactJobView {
        job_id: id.to_string(),
        idempotency_key,
        channel,
        provider,
        status,
        attempt_count,
        max_attempts,
        provider_message_id,
        last_error,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_is_parameterized_and_names_only_canonical_columns() {
        assert!(GET_CONTACT_JOB.contains("WHERE id = $1"));
        assert!(GET_CONTACT_JOB.contains("public.contact_jobs"));
        assert!(!GET_CONTACT_JOB.contains("SELECT *"));
    }

    #[test]
    fn stored_statuses_follow_the_canonical_contract() {
        for status in ["queued", "leased", "retry", "sent", "dead", "cancelled"] {
            let row = (
                Uuid::nil(),
                "idempotency-key".to_owned(),
                "email".to_owned(),
                "sendgrid".to_owned(),
                status.to_owned(),
                0, 8, None, None,
            );
            assert!(decode_contact_job(row).is_ok(), "{status}");
        }
        let invalid = (
            Uuid::nil(), "key".to_owned(), "email".to_owned(),
            "sendgrid".to_owned(), "unknown".to_owned(), 0, 8, None, None,
        );
        assert!(matches!(decode_contact_job(invalid), Err(ReadError::InvalidStoredData)));
    }
}
