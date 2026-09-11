#![forbid(unsafe_code)]

use crate::fanwaave_config::{
    resolve_fanwaave_config, validate_fanwaave_config, FanwaaveConfig, FanwaaveConfigError,
    ResolvedFanwaaveConfig,
};
use flags2env::BundledFlags2Env;
use std::collections::BTreeMap;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum FanwaaveFlags2EnvError {
    #[error(transparent)]
    Config(#[from] FanwaaveConfigError),
    #[error("flags-2-env config audit failed")]
    Audit,
    #[error("flags-2-env argv parsing failed")]
    Parse,
    #[error("flags-2-env rejected unknown or invalid CLI arguments")]
    Arguments,
}

/// Audit and parse argv through the official bundled `flags-2-env` runtime,
/// returning argv-derived overrides only. Defaults and ambient environment
/// values are intentionally excluded so Fanwaave can preserve the contract
/// precedence `argv > environment > non-secret Fanwaave default`.
///
/// This compatibility entry point resolves the authored repository-root
/// `.cli-flags.toml` relative to the current working directory. Code that
/// already knows the location of `.fanwaave-cfg.toml` should prefer
/// [`argv_overrides_from_flags2env_at`] so package/test working directories do
/// not accidentally change which contract is audited.
pub fn argv_overrides_from_flags2env(
    config: &FanwaaveConfig,
    argv: &[String],
) -> Result<BTreeMap<String, String>, FanwaaveFlags2EnvError> {
    argv_overrides_from_flags2env_at(config, argv, Path::new("."))
}

/// Audit and parse argv using the canonical `.cli-flags.toml` located under
/// `contract_root`.
///
/// `validate_fanwaave_config` still owns the authored contract name and rejects
/// anything except repository-root `.cli-flags.toml`; this function only makes
/// the filesystem resolution base explicit. That keeps an admitted relative
/// contract deterministic when Cargo, a test harness, or another launcher runs
/// from a nested package directory.
pub fn argv_overrides_from_flags2env_at(
    config: &FanwaaveConfig,
    argv: &[String],
    contract_root: &Path,
) -> Result<BTreeMap<String, String>, FanwaaveFlags2EnvError> {
    validate_fanwaave_config(config)?;
    let parser = BundledFlags2Env::new();
    let contract_path = contract_root.join(&config.flags2env.contract);
    let contract = contract_path.to_string_lossy();

    parser
        .audit_config(Some(contract.as_ref()))
        .map_err(|_| FanwaaveFlags2EnvError::Audit)?;
    let parsed = parser
        .parse_structured(argv, Some(contract.as_ref()))
        .map_err(|_| FanwaaveFlags2EnvError::Parse)?;

    if !parsed.unknown_options.is_empty() || !parsed.errors.is_empty() {
        return Err(FanwaaveFlags2EnvError::Arguments);
    }

    Ok(parsed.provided_flags.into_iter().collect())
}

/// Resolve a Fanwaave config directly from process-style argv without
/// duplicating CLI parsing or precedence logic in Fanwaave.
///
/// Secret bindings remain environment-only: the underlying Fanwaave resolver
/// rejects any secret key that appears in argv-derived overrides.
pub fn resolve_fanwaave_config_from_argv(
    config: &FanwaaveConfig,
    ambient: &BTreeMap<String, String>,
    argv: &[String],
) -> Result<ResolvedFanwaaveConfig, FanwaaveFlags2EnvError> {
    resolve_fanwaave_config_from_argv_at(config, ambient, argv, Path::new("."))
}

/// Resolve a Fanwaave config while anchoring its admitted flags2env contract to
/// an explicit repository/config root. Callers loading `.fanwaave-cfg.toml`
/// from a known path should pass that file's parent directory.
pub fn resolve_fanwaave_config_from_argv_at(
    config: &FanwaaveConfig,
    ambient: &BTreeMap<String, String>,
    argv: &[String],
    contract_root: &Path,
) -> Result<ResolvedFanwaaveConfig, FanwaaveFlags2EnvError> {
    let argv_overrides = argv_overrides_from_flags2env_at(config, argv, contract_root)?;
    Ok(resolve_fanwaave_config(config, ambient, &argv_overrides)?)
}
