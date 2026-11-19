#![forbid(unsafe_code)]
//! Authenticated contact-job reads over a real SQLx PostgreSQL pool.
//! This module exports no arbitrary SQL, migration, or mutation method.
//! Errors intentionally never expose database URLs or secret-bearing text.

use fanwaave_interfaces::contact_read::{
    ContactReadGrantClaims, CONTACT_READ_AUDIENCE, CONTACT_READ_ISSUER,
};
use jsonwebtoken::{decode, decode_header, Algorithm, DecodingKey, Validation};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::{
    collections::BTreeMap,
    fmt,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
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
    #[error("valid trusted contact read verification keys are required")]
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

// Named SQL projection avoids positional column-order ambiguity and makes
// missing/renamed fields a type-safe SQLx decode failure.
#[derive(sqlx::FromRow)]
struct ContactJobRow {
    idempotency_key: String,
    channel: String,
    provider: String,
    status: String,
    attempt_count: i32,
    max_attempts: i32,
    provider_message_id: Option<String>,
    last_error: Option<String>,
}

/// One configured, trusted HMAC verifier key (not an untrusted JWT header).
/// The trusted auth service, not this library, issues job-specific grants.
pub struct ReadVerifierKey<'a> {
    pub kid: &'a str,
    pub secret: &'a [u8],
}

const MAX_GRANT_BYTES: usize = 4_096;
const MAX_GRANT_SECONDS: i64 = 300;
const MAX_KEY_COUNT: usize = 8;

/// A verified resource-bound credential, not a tenant-level SQL policy.
/// A future tenant-specific contract needs separately reviewed authority.
#[derive(Clone)]
pub struct ReadContext {
    pool: PgPool,
    trusted_keys: Arc<BTreeMap<String, DecodingKey>>,
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
    pub async fn connect(config: &CoreConfig, keys: &[ReadVerifierKey<'_>]) -> Result<Self, ReadError> {
        if !config.read_only {
            return Err(ReadError::NotReadOnly);
        }
        if keys.is_empty() || keys.len() > MAX_KEY_COUNT {
            return Err(ReadError::InvalidReadCredential);
        }
        let mut trusted_keys = BTreeMap::new();
        for key in keys {
            if key.kid.is_empty()
                || key.kid.len() > 64
                || !key
                    .kid
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
                || key.secret.len() < 32
                || trusted_keys
                    .insert(key.kid.to_owned(), DecodingKey::from_secret(key.secret))
                    .is_some()
            {
                return Err(ReadError::InvalidReadCredential);
            }
        }
        if config.flavor != DatabaseFlavor::PostgreSql {
            return Err(ReadError::UnsupportedFlavor);
        }

        let pool = PgPoolOptions::new()
            .max_connections(4)
            .acquire_timeout(Duration::from_secs(5))
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

        // The URL classifier is only a hint: verify actual connected engine.
        let (version,): (String,) = sqlx::query_as("SELECT version()")
            .fetch_one(&pool)
            .await
            .map_err(|_| ReadError::Database)?;
        if !version.starts_with("PostgreSQL ") {
            return Err(ReadError::UnsupportedFlavor);
        }

        // Reject elevated roles and mutation/DDL privileges across *all*
        // non-system schemas, not merely the target contact_jobs table.
        let (unsafe_role,): (bool,) = sqlx::query_as(
            "SELECT
                 EXISTS (
                   SELECT 1 FROM pg_roles
                   WHERE rolname = current_user
                     AND (rolsuper OR rolcreatedb OR rolcreaterole
                          OR rolreplication OR rolbypassrls)
                 )
                 OR has_database_privilege(current_database(), 'CREATE')
                 OR EXISTS (
                   SELECT 1 FROM pg_namespace n
                   WHERE n.nspname <> 'information_schema'
                     AND n.nspname !~ '^pg_'
                     AND has_schema_privilege(n.oid, 'CREATE')
                 )
                 OR EXISTS (
                   SELECT 1 FROM pg_class c
                   JOIN pg_namespace n ON n.oid = c.relnamespace
                   WHERE n.nspname <> 'information_schema'
                     AND n.nspname !~ '^pg_'
                     AND c.relkind IN ('r', 'p', 'v', 'm', 'f')
                     AND has_table_privilege(c.oid,
                        'INSERT, UPDATE, DELETE, TRUNCATE, REFERENCES, TRIGGER')
                 )",
        )
        .fetch_one(&pool)
        .await
        .map_err(|_| ReadError::Database)?;
        if unsafe_role {
            return Err(ReadError::ExcessivePrivileges);
        }

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
            trusted_keys: Arc::new(trusted_keys),
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
        self.authorize(authorization, job_id)?;
        let row = sqlx::query_as::<_, ContactJobRow>(
            "SELECT idempotency_key, channel, provider, status, attempt_count, \
                max_attempts, provider_message_id, last_error \
             FROM public.contact_jobs WHERE id = $1",
        )
        .bind(job_id)
        .fetch_optional(&self.pool)
        .await
        .map_err(|_| ReadError::Database)?;

        row.map(|job| {
            if job.attempt_count < 0 || job.max_attempts <= 0 {
                return Err(ReadError::InvalidData);
            }
            Ok(ContactJobView {
                job_id: job_id.to_string(),
                idempotency_key: job.idempotency_key,
                channel: parse_channel(&job.channel)?,
                provider: parse_provider(&job.provider)?,
                status: parse_status(&job.status)?,
                attempt_count: job.attempt_count,
                max_attempts: job.max_attempts,
                provider_message_id: job.provider_message_id,
                last_error: job.last_error,
            })
        })
        .transpose()
    }

    fn authorize(&self, header: &str, requested_job: Uuid) -> Result<(), ReadError> {
        let supplied = header
            .strip_prefix("Bearer ")
            .ok_or(ReadError::Unauthorized)?;
        if supplied.is_empty() || supplied.len() > MAX_GRANT_BYTES {
            return Err(ReadError::Unauthorized);
        }
        let jose = decode_header(supplied).map_err(|_| ReadError::Unauthorized)?;
        if jose.alg != Algorithm::HS256 {
            return Err(ReadError::Unauthorized);
        }
        let kid = jose.kid.ok_or(ReadError::Unauthorized)?;
        let key = self.trusted_keys.get(&kid).ok_or(ReadError::Unauthorized)?;
        let mut validation = Validation::new(Algorithm::HS256);
        validation.set_issuer(&[CONTACT_READ_ISSUER]);
        validation.set_audience(&[CONTACT_READ_AUDIENCE]);
        validation.validate_nbf = true;
        validation.leeway = 0;
        validation.required_spec_claims = [
            "iss", "aud", "sub", "scope", "job_id", "iat", "nbf", "exp", "jti",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let verified = decode::<ContactReadGrantClaims>(supplied, key, &validation)
            .map_err(|_| ReadError::Unauthorized)?;
        let claims = verified.claims;
        claims.validate_shape().map_err(|_| ReadError::Unauthorized)?;
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| ReadError::Unauthorized)?
            .as_secs() as i64;
        let life = claims.exp.checked_sub(claims.iat);
        if life.is_none_or(|duration| duration <= 0 || duration > MAX_GRANT_SECONDS)
            || claims.iat > now.saturating_add(30)
            || claims.exp <= now
            || claims.nbf > now
            || claims.job_id != requested_job.to_string()
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
        assert!(matches!(
            parse_provider("twilio"),
            Ok(ContactProvider::Twilio)
        ));
        assert!(matches!(
            parse_status("cancelled"),
            Ok(ContactJobStatus::Cancelled)
        ));
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
