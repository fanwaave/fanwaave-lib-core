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
        // Match the actual parsed host, not an arbitrary substring in the
        // password, database name or query parameters.
        let parsed = url::Url::parse(&database_url).map_err(|_| CoreError::InvalidDatabaseUrl)?;
        if !matches!(parsed.scheme(), "postgres" | "postgresql") || parsed.host_str().is_none() {
            return Err(CoreError::InvalidDatabaseUrl);
        }
        let hostname = parsed.host_str().expect("hostname checked above");
        let flavor = if hostname.to_ascii_lowercase().contains("cockroach") {
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
    fn only_the_database_hostname_determines_flavor() {
        let postgres = CoreConfig::from_values(
            "postgres://user:cockroach@postgres.example/tenant_cockroach",
            None,
        )
        .expect("valid PostgreSQL URL");
        assert_eq!(postgres.flavor, DatabaseFlavor::PostgreSql);

        let cockroach = CoreConfig::from_values(
            "postgresql://user:password@eu.cockroachdb.example/app",
            None,
        )
        .expect("valid CockroachDB URL");
        assert_eq!(cockroach.flavor, DatabaseFlavor::CockroachDb);
    }

    #[test]
    fn invalid_database_urls_fail_before_a_config_is_constructed() {
        assert!(matches!(
            CoreConfig::from_values("mysql://db/app", Some("0")),
            Err(CoreError::InvalidDatabaseUrl)
        ));
        for url in ["postgres://", "postgresql:///no-host", "not a URL"] {
            assert!(matches!(
                CoreConfig::from_values(url, None),
                Err(CoreError::InvalidDatabaseUrl)
            ));
        }
    }
}
