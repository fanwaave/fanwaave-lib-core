#![forbid(unsafe_code)]

use serde::Deserialize;
use serde_json::Value as JsonValue;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use thiserror::Error;
use url::Url;

pub const FANWAAVE_CONFIG_FILENAME: &str = ".fanwaave-cfg.toml";
pub const FANWAAVE_CONFIG_CONTRACT_REPOSITORY: &str = "fanwaave/fanwaave-interfaces";
pub const FANWAAVE_CONFIG_CONTRACT_REVISION: &str =
    "e27695091a5b8276543a6f435156a25043f297a9";
pub const FANWAAVE_CONFIG_TJSV_REVISION: &str =
    "4a5d049218adc2740d4cf78f612caf7f38f6f64c";

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FanwaaveMode {
    Client,
    Server,
    Hybrid,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum EnvKind {
    String,
    Bool,
    Integer,
    Double,
    Json,
    Url,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
pub enum Flags2EnvPrecedence {
    #[serde(rename = "argv-over-env")]
    ArgvOverEnv,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Flags2EnvConfig {
    pub contract: String,
    pub require_audit: bool,
    pub precedence: Flags2EnvPrecedence,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnvBinding {
    pub name: String,
    pub key: String,
    pub kind: EnvKind,
    pub required: bool,
    pub secret: bool,
    #[serde(rename = "default")]
    pub default_value: Option<String>,
    pub description: Option<String>,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FanwaaveClientConfig {
    pub enabled: bool,
    pub api_base_url_binding: Option<String>,
    pub tenant_id_binding: Option<String>,
    pub auth_token_binding: Option<String>,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FanwaaveServerConfig {
    pub enabled: bool,
    pub bind_addr_binding: Option<String>,
    pub database_url_binding: Option<String>,
    pub push_provider_binding: Option<String>,
}

#[derive(Clone, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct FanwaaveConfig {
    pub version: u32,
    pub mode: FanwaaveMode,
    pub strict: bool,
    pub flags2env: Flags2EnvConfig,
    pub env: Vec<EnvBinding>,
    pub client: Option<FanwaaveClientConfig>,
    pub server: Option<FanwaaveServerConfig>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ConfigValue {
    String(String),
    Bool(bool),
    Integer(i64),
    Double(f64),
    Json(JsonValue),
    Url(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueSource {
    Argv,
    Environment,
    Default,
}

#[derive(Clone, PartialEq)]
pub struct ResolvedBinding {
    env_key: String,
    value: ConfigValue,
    source: ValueSource,
    secret: bool,
}

impl ResolvedBinding {
    pub fn env_key(&self) -> &str {
        &self.env_key
    }

    pub fn value(&self) -> &ConfigValue {
        &self.value
    }

    pub fn source(&self) -> ValueSource {
        self.source
    }

    pub fn is_secret(&self) -> bool {
        self.secret
    }
}

impl fmt::Debug for ResolvedBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("ResolvedBinding");
        debug.field("env_key", &self.env_key);
        debug.field("source", &self.source);
        debug.field("secret", &self.secret);
        if self.secret {
            debug.field("value", &"[REDACTED]");
        } else {
            debug.field("value", &self.value);
        }
        debug.finish()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedFanwaaveConfig {
    mode: FanwaaveMode,
    values: BTreeMap<String, ResolvedBinding>,
}

impl ResolvedFanwaaveConfig {
    pub fn mode(&self) -> FanwaaveMode {
        self.mode
    }

    pub fn binding(&self, name: &str) -> Option<&ResolvedBinding> {
        self.values.get(name)
    }

    pub fn bindings(&self) -> &BTreeMap<String, ResolvedBinding> {
        &self.values
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum FanwaaveConfigError {
    #[error("invalid .fanwaave-cfg.toml: {0}")]
    Toml(String),
    #[error("unsupported Fanwaave config version {0}")]
    UnsupportedVersion(u32),
    #[error("Fanwaave config strict mode must be enabled")]
    StrictModeRequired,
    #[error("flags-2-env audit must be enabled")]
    FlagsAuditRequired,
    #[error("flags-2-env contract must be repository-root .cli-flags.toml")]
    UnsafeFlagsContract,
    #[error("duplicate Fanwaave binding name: {0}")]
    DuplicateBindingName(String),
    #[error("duplicate Fanwaave environment key: {0}")]
    DuplicateEnvironmentKey(String),
    #[error("invalid Fanwaave binding name: {0}")]
    InvalidBindingName(String),
    #[error("invalid Fanwaave environment key: {0}")]
    InvalidEnvironmentKey(String),
    #[error("secret binding may not declare a plaintext default: {0}")]
    SecretDefault(String),
    #[error("secret binding may not be supplied through argv: {0}")]
    SecretFromArgv(String),
    #[error("required Fanwaave binding is unresolved: {0}")]
    MissingRequiredBinding(String),
    #[error("Fanwaave role is inconsistent with mode: {0}")]
    ModeRoleMismatch(String),
    #[error("unknown Fanwaave binding reference {binding} at {field}")]
    UnknownBindingReference { field: String, binding: String },
    #[error("Fanwaave binding {binding} at {field} must use kind {expected:?}")]
    BindingKindMismatch {
        field: String,
        binding: String,
        expected: EnvKind,
    },
    #[error("Fanwaave binding {binding} at {field} must be marked secret")]
    SensitiveBindingMustBeSecret { field: String, binding: String },
    #[error("invalid boolean value for Fanwaave binding: {0}")]
    InvalidBoolean(String),
    #[error("invalid integer value for Fanwaave binding: {0}")]
    InvalidInteger(String),
    #[error("invalid double value for Fanwaave binding: {0}")]
    InvalidDouble(String),
    #[error("invalid JSON value for Fanwaave binding: {0}")]
    InvalidJson(String),
    #[error("invalid URL value for Fanwaave binding: {0}")]
    InvalidUrl(String),
}

pub fn parse_fanwaave_config(input: &str) -> Result<FanwaaveConfig, FanwaaveConfigError> {
    let config = toml::from_str::<FanwaaveConfig>(input)
        .map_err(|error| FanwaaveConfigError::Toml(error.to_string()))?;
    validate_fanwaave_config(&config)?;
    Ok(config)
}

pub fn validate_fanwaave_config(config: &FanwaaveConfig) -> Result<(), FanwaaveConfigError> {
    if config.version != 1 {
        return Err(FanwaaveConfigError::UnsupportedVersion(config.version));
    }
    if !config.strict {
        return Err(FanwaaveConfigError::StrictModeRequired);
    }
    if !config.flags2env.require_audit {
        return Err(FanwaaveConfigError::FlagsAuditRequired);
    }
    if config.flags2env.contract != ".cli-flags.toml" {
        return Err(FanwaaveConfigError::UnsafeFlagsContract);
    }

    let mut names = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut by_name = BTreeMap::new();

    for binding in &config.env {
        if !is_binding_name(&binding.name) {
            return Err(FanwaaveConfigError::InvalidBindingName(binding.name.clone()));
        }
        if !is_environment_key(&binding.key) {
            return Err(FanwaaveConfigError::InvalidEnvironmentKey(binding.key.clone()));
        }
        if !names.insert(binding.name.as_str()) {
            return Err(FanwaaveConfigError::DuplicateBindingName(binding.name.clone()));
        }
        if !keys.insert(binding.key.as_str()) {
            return Err(FanwaaveConfigError::DuplicateEnvironmentKey(binding.key.clone()));
        }
        if binding.secret && binding.default_value.is_some() {
            return Err(FanwaaveConfigError::SecretDefault(binding.name.clone()));
        }
        by_name.insert(binding.name.as_str(), binding);
    }

    validate_mode(config)?;

    if let Some(client) = &config.client {
        validate_reference(
            &by_name,
            "client.api_base_url_binding",
            client.api_base_url_binding.as_deref(),
            EnvKind::Url,
            false,
        )?;
        validate_reference(
            &by_name,
            "client.tenant_id_binding",
            client.tenant_id_binding.as_deref(),
            EnvKind::String,
            false,
        )?;
        validate_reference(
            &by_name,
            "client.auth_token_binding",
            client.auth_token_binding.as_deref(),
            EnvKind::String,
            true,
        )?;
    }

    if let Some(server) = &config.server {
        validate_reference(
            &by_name,
            "server.bind_addr_binding",
            server.bind_addr_binding.as_deref(),
            EnvKind::String,
            false,
        )?;
        validate_reference(
            &by_name,
            "server.database_url_binding",
            server.database_url_binding.as_deref(),
            EnvKind::Url,
            true,
        )?;
        validate_reference(
            &by_name,
            "server.push_provider_binding",
            server.push_provider_binding.as_deref(),
            EnvKind::String,
            false,
        )?;
    }

    Ok(())
}

pub fn merge_environment(
    ambient: &BTreeMap<String, String>,
    argv_overrides: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut merged = ambient.clone();
    for (key, value) in argv_overrides {
        merged.insert(key.clone(), value.clone());
    }
    merged
}

pub fn resolve_fanwaave_config(
    config: &FanwaaveConfig,
    ambient: &BTreeMap<String, String>,
    argv_overrides: &BTreeMap<String, String>,
) -> Result<ResolvedFanwaaveConfig, FanwaaveConfigError> {
    validate_fanwaave_config(config)?;

    let mut values = BTreeMap::new();
    for binding in &config.env {
        if binding.secret && argv_overrides.contains_key(&binding.key) {
            return Err(FanwaaveConfigError::SecretFromArgv(binding.name.clone()));
        }

        let resolved = if let Some(value) = argv_overrides.get(&binding.key) {
            Some((value.as_str(), ValueSource::Argv))
        } else if let Some(value) = ambient.get(&binding.key) {
            Some((value.as_str(), ValueSource::Environment))
        } else if let Some(value) = binding.default_value.as_deref() {
            Some((value, ValueSource::Default))
        } else {
            None
        };

        let Some((raw_value, source)) = resolved else {
            if binding.required {
                return Err(FanwaaveConfigError::MissingRequiredBinding(
                    binding.name.clone(),
                ));
            }
            continue;
        };

        if binding.required && raw_value.is_empty() {
            return Err(FanwaaveConfigError::MissingRequiredBinding(
                binding.name.clone(),
            ));
        }

        let value = coerce_value(binding, raw_value)?;
        values.insert(
            binding.name.clone(),
            ResolvedBinding {
                env_key: binding.key.clone(),
                value,
                source,
                secret: binding.secret,
            },
        );
    }

    Ok(ResolvedFanwaaveConfig {
        mode: config.mode,
        values,
    })
}

fn validate_mode(config: &FanwaaveConfig) -> Result<(), FanwaaveConfigError> {
    let client_enabled = config.client.as_ref().is_some_and(|client| client.enabled);
    let server_enabled = config.server.as_ref().is_some_and(|server| server.enabled);

    match config.mode {
        FanwaaveMode::Client if !client_enabled || server_enabled => Err(
            FanwaaveConfigError::ModeRoleMismatch(
                "client mode requires client.enabled=true and server disabled".to_owned(),
            ),
        ),
        FanwaaveMode::Server if !server_enabled || client_enabled => Err(
            FanwaaveConfigError::ModeRoleMismatch(
                "server mode requires server.enabled=true and client disabled".to_owned(),
            ),
        ),
        FanwaaveMode::Hybrid if !client_enabled || !server_enabled => Err(
            FanwaaveConfigError::ModeRoleMismatch(
                "hybrid mode requires both client.enabled=true and server.enabled=true".to_owned(),
            ),
        ),
        _ => Ok(()),
    }
}

fn validate_reference(
    by_name: &BTreeMap<&str, &EnvBinding>,
    field: &str,
    reference: Option<&str>,
    expected_kind: EnvKind,
    must_be_secret: bool,
) -> Result<(), FanwaaveConfigError> {
    let Some(reference) = reference else {
        return Ok(());
    };
    let Some(binding) = by_name.get(reference) else {
        return Err(FanwaaveConfigError::UnknownBindingReference {
            field: field.to_owned(),
            binding: reference.to_owned(),
        });
    };
    if binding.kind != expected_kind {
        return Err(FanwaaveConfigError::BindingKindMismatch {
            field: field.to_owned(),
            binding: reference.to_owned(),
            expected: expected_kind,
        });
    }
    if must_be_secret && !binding.secret {
        return Err(FanwaaveConfigError::SensitiveBindingMustBeSecret {
            field: field.to_owned(),
            binding: reference.to_owned(),
        });
    }
    Ok(())
}

fn coerce_value(binding: &EnvBinding, raw_value: &str) -> Result<ConfigValue, FanwaaveConfigError> {
    match binding.kind {
        EnvKind::String => Ok(ConfigValue::String(raw_value.to_owned())),
        EnvKind::Bool => match raw_value.to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" | "on" => Ok(ConfigValue::Bool(true)),
            "false" | "0" | "no" | "off" => Ok(ConfigValue::Bool(false)),
            _ => Err(FanwaaveConfigError::InvalidBoolean(binding.name.clone())),
        },
        EnvKind::Integer => raw_value
            .parse::<i64>()
            .map(ConfigValue::Integer)
            .map_err(|_| FanwaaveConfigError::InvalidInteger(binding.name.clone())),
        EnvKind::Double => raw_value
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(ConfigValue::Double)
            .ok_or_else(|| FanwaaveConfigError::InvalidDouble(binding.name.clone())),
        EnvKind::Json => serde_json::from_str::<JsonValue>(raw_value)
            .map(ConfigValue::Json)
            .map_err(|_| FanwaaveConfigError::InvalidJson(binding.name.clone())),
        EnvKind::Url => Url::parse(raw_value)
            .map(|_| ConfigValue::Url(raw_value.to_owned()))
            .map_err(|_| FanwaaveConfigError::InvalidUrl(binding.name.clone())),
    }
}

fn is_binding_name(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(first) if first.is_ascii_lowercase())
        && characters.all(|character| character.is_ascii_lowercase() || character.is_ascii_digit() || character == '_')
        && value.len() <= 64
}

fn is_environment_key(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(first) if first.is_ascii_uppercase() || first == '_')
        && characters.all(|character| character.is_ascii_uppercase() || character.is_ascii_digit() || character == '_')
        && value.len() <= 128
}
