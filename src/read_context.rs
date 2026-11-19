#![forbid(unsafe_code)]
//! Authenticated contact-job reads over a real SQLx PostgreSQL pool.
//! This module exports no arbitrary SQL, migration, or mutation method.
//! Errors intentionally never expose database URLs or secret-bearing text.

use std::{fmt, sync::Arc};
use sqlx::{postgres::PgPoolOptions, PgPool};
use subtle::ConstantTimeEq;
use uuid::Uuid;

use crate::{
    config::CoreConfig,
    contact::{ContactChannel, ContactJobStatus, ContactJobView, ContactProvider},
    flavor::DatabaseFlavor,
};

#[derive(Debug, Clone, Copy, Eq, PartialEq, thiserror::Error)]
pub enum ReadError {
    #[error("read-only database configuration is required")]
    NotReadOnly,
    #[error("a strong dedicated read credential is required")]
    InvalidReadCredential,
    #[error("database flavor is not verified for read-only operation")]
    UnsupportedFlavor,
    #[error("contact read access is not authorized")]
    Unauthorized,
    #[error("database reader lacks the required SELECT privilege")]
    MissingReadPrivilege,
    #[error("database reader has prohibited write privileges")]
    ExcessivePrivileges,
    #[error("contact job schema does not support the required read projection")]
    IncompatibleSchema,
    #[error("contact database operation failed")]
    Database,
    #[error("contact database returned an invalid job projection")]
    InvalidData,
}

/// A service-scoped read credential, NOT a per-tenant authorization system.
/// A future tenant-specific contract needs separately reviewed authority.
#[derive(Clone)]
pub struct ReadContext {
    pool: PgPool,
    read_secret: Arc<str>,
}

impl fmt::Debug for ReadContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ReadContext").finish_non_exhaustive()
    }
}

impl ReadContext {
    /// Create a real read-only pool using a dedicated SELECT-only DB role.
    ///
    /// Each new connection has read-only transactions by default. At startup
    /// the role must have SELECT but no INSERT/UPDATE/DELETE/TRUNCATE/REFERENCES/
    /// TRIGGER permission on public.contact_jobs. The database administrator
    /// must additionally withhold write/DDL permissions on all other objects.
    /// Only PostgreSQL is admitted; CockroachDB needs separate executed proof.
    ///
    /// # Errors
    /// A typed, credential-safe failure on policy, connectivity or schema.
    pub async fn connect(config: &CoreConfig, read_secret: &str) -> Result<Self, ReadError> {
        if !config.read_only {
            return Err(ReadError::NotReadOnly);
        }
        if read_secret.trim().len() < 32 {
            return Err(ReadError::InvalidReadCredential);
        }
        if config.flavor != DatabaseFlavor::PostgreSql {
            return Err(ReadError::UnsupportedFlavor);
        }

        let pool = PgPoolOptions::new()
            .max_connections(4)
            .after_connect(|connection, _metadata| {
                Box::pin(async move {
                    sqlx::query("SET default_transaction_read_only = on")
                        .execute(connection)
                        .await?;
                    Ok(())
                })
            })
            .connect(&config.database_url)
            .await
            .map_err(|_| ReadError::Database)?;

        let (can_read, can_modify): (bool, bool) = sqlx::query_as(
            "SELECT \
                has_table_privilege(current_user, 'public.contact_jobs', 'SELECT'), \
                has_table_privilege(current_user, 'public.contact_jobs', \
                    'INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER')",
        )
        .fetch_one(&pool)
        .await
        .map_err(|_| ReadError::Database)?;
        if !can_read {
            return Err(ReadError::MissingReadPrivilege);
        }
        if can_modify {
            return Err(ReadError::ExcessivePrivileges);
        }

        // Parse-check exact canonical column names before allowing any read.
        sqlx::query(
            "SELECT id, idempotency_key, channel, provider, status, \
                attempt_count, max_attempts, provider_message_id, last_error \
             FROM public.contact_jobs WHERE false",
        )
        .execute(&pool)
        .await
        .map_err(|_| ReadError::IncompatibleSchema)?;

        Ok(Self {
            pool,
            read_secret: Arc::from(read_secret),
        })
    }

    /// Fetch the TypeSpec ContactJobView by UUID, authorizing first.
    /// Unknown IDs return None and never a fabricated record.
    ///
    /// # Errors
    /// Unauthorized, Database, or InvalidData, with no raw SQL error text.
    pub async fn get_contact_job(
        &self,
        job_id: Uuid,
        authorization: &str,
    ) -> Result<Option<ContactJobView>, ReadError> {
        self.authorize(authorization)?;
        let row: Option<(String, String, String, String, i32, i32, Option<String>, Option<String>)> =
            sqlx::query_as(
                "SELECT idempotency_key, channel, provider, status, attempt_count, \
                    max_attempts, provider_message_id, last_error \
                 FROM public.contact_jobs WHERE id = $1",
            )
            .bind(job_id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|_| ReadError::Database)?;

        row.map(|(key, channel, provider, status, attempts, max_attempts, message_id, last_error)| {
            if attempts < 0 || max_attempts <= 0 {
                return Err(ReadError::InvalidData);
            }
            Ok(ContactJobView {
                job_id: job_id.to_string(),
                idempotency_key: key,
                channel: parse_channel(&channel)?,
                provider: parse_provider(&provider)?,
                status: parse_status(&status)?,
                attempt_count: attempts,
                max_attempts,
                provider_message_id: message_id,
                last_error,
            })
        })
        .transpose()
    }

    fn authorize(&self, header: &str) -> Result<(), ReadError> {
        let supplied = header.strip_prefix("Bearer ").ok_or(ReadError::Unauthorized)?;
        if supplied.is_empty() || !bool::from(supplied.as_bytes().ct_eq(self.read_secret.as_bytes()))
        {
            return Err(ReadError::Unauthorized);
        }
        Ok(())
    }
}

fn parse_channel(raw: &str) -> Result<ContactChannel, ReadError> {
    match raw {
        "email" => Ok(ContactChannel::Email),
        "sms" => Ok(ContactChannel::Sms),
        _ => Err(ReadError::InvalidData),
    }
}

fn parse_provider(raw: &str) -> Result<ContactProvider, ReadError> {
    match raw {
        "sendgrid" => Ok(ContactProvider::Sendgrid),
        "twilio" => Ok(ContactProvider::Twilio),
        _ => Err(ReadError::InvalidData),
    }
}

fn parse_status(raw: &str) -> Result<ContactJobStatus, ReadError> {
    match raw {
        "queued" => Ok(ContactJobStatus::Queued),
        "leased" => Ok(ContactJobStatus::Leased),
        "retry" => Ok(ContactJobStatus::Retry),
        "sent" => Ok(ContactJobStatus::Sent),
        "dead" => Ok(ContactJobStatus::Dead),
        "cancelled" => Ok(ContactJobStatus::Cancelled),
        _ => Err(ReadError::InvalidData),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_enums_match_canonical_contract() {
        assert!(matches!(parse_channel("email"), Ok(ContactChannel::Email)));
        assert!(matches!(parse_provider("twilio"), Ok(ContactProvider::Twilio)));
        assert!(matches!(parse_status("cancelled"), Ok(ContactJobStatus::Cancelled)));
        assert_eq!(parse_status("wrong"), Err(ReadError::InvalidData));
        assert_eq!(parse_channel("Email"), Err(ReadError::InvalidData));
        assert_eq!(parse_provider("internal"), Err(ReadError::InvalidData));
    }

    #[test]
    fn errors_do_not_contain_credentials() {
        for error in [
            ReadError::Unauthorized,
            ReadError::Database,
            ReadError::InvalidData,
            ReadError::ExcessivePrivileges,
        ] {
            assert!(!format!("{error:?}").contains("token=secret"));
            assert!(!error.to_string().contains("postgres://"));
        }
    }
}
