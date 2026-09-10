#![forbid(unsafe_code)]

use crate::fanwaave_config::{
    resolve_fanwaave_config, validate_fanwaave_config, FanwaaveConfig, FanwaaveConfigError,
    ResolvedFanwaaveConfig,
};
use flags2env::BundledFlags2Env;
use std::collections::BTreeMap;
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
pub fn argv_overrides_from_flags2env(
    config: &FanwaaveConfig,
    argv: &[String],
) -> Result<BTreeMap<String, String>, FanwaaveFlags2EnvError> {
    validate_fanwaave_config(config)?;
    let parser = BundledFlags2Env::new();
    let contract = config.flags2env.contract.as_str();

    parser
        .audit_config(Some(contract))
        .map_err(|_| FanwaaveFlags2EnvError::Audit)?;
    let parsed = parser
        .parse_structured(argv, Some(contract))
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
    let argv_overrides = argv_overrides_from_flags2env(config, argv)?;
    Ok(resolve_fanwaave_config(config, ambient, &argv_overrides)?)
}
