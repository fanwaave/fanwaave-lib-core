#![forbid(unsafe_code)]

use crate::error::CoreError;
use crate::flavor::DatabaseFlavor;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreConfig {
    pub database_url: String,
    pub flavor: DatabaseFlavor,
    pub read_only: bool,
}

impl CoreConfig {
    /// Builds a fully resolved configuration from explicit values.
    ///
    /// This is the functional core of configuration resolution: it performs no
    /// process-environment reads and returns one complete value whose fields are
    /// derived only from its inputs.
    pub fn from_values(
        database_url: impl Into<String>,
        read_only_env: Option<&str>,
    ) -> Result<Self, CoreError> {
        let database_url = database_url.into();
        if !(database_url.starts_with("postgres://") || database_url.starts_with("postgresql://")) {
            return Err(CoreError::InvalidDatabaseUrl);
        }
        let flavor = if database_url.contains("cockroach") {
            DatabaseFlavor::CockroachDb
        } else {
            DatabaseFlavor::PostgreSql
        };
        Ok(Self {
            database_url,
            flavor,
            read_only: read_only_env != Some("0"),
        })
    }

    /// Reads the process boundary once, then delegates all policy to
    /// [`Self::from_values`]. Keeping this shell tiny makes configuration logic
    /// deterministic and directly testable without mutating global env state.
    pub fn from_env() -> Result<Self, CoreError> {
        let database_url =
            std::env::var("FANWAAVE_DATABASE_URL").map_err(|_| CoreError::InvalidDatabaseUrl)?;
        let read_only = std::env::var("FANWAAVE_DB_READ_ONLY").ok();
        Self::from_values(database_url, read_only.as_deref())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_values_resolve_without_process_state() {
        let postgres =
            CoreConfig::from_values("postgres://db/app", None).expect("valid PostgreSQL config");
        assert_eq!(
            postgres,
            CoreConfig {
                database_url: "postgres://db/app".to_owned(),
                flavor: DatabaseFlavor::PostgreSql,
                read_only: true,
            }
        );

        let cockroach = CoreConfig::from_values("postgresql://cockroach/app", Some("0"))
            .expect("valid CockroachDB config");
        assert_eq!(
            cockroach,
            CoreConfig {
                database_url: "postgresql://cockroach/app".to_owned(),
                flavor: DatabaseFlavor::CockroachDb,
                read_only: false,
            }
        );
    }

    #[test]
    fn invalid_database_urls_fail_before_a_config_is_constructed() {
        assert!(matches!(
            CoreConfig::from_values("mysql://db/app", Some("0")),
            Err(CoreError::InvalidDatabaseUrl)
        ));
    }
}
