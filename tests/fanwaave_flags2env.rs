use fanwaave_lib_core::fanwaave_config::{parse_fanwaave_config, ConfigValue, ValueSource};
use fanwaave_lib_core::fanwaave_flags2env::{
    resolve_fanwaave_config_from_argv, resolve_fanwaave_config_from_argv_at,
    FanwaaveFlags2EnvError,
};
use std::collections::BTreeMap;

fn client_config() -> &'static str {
    r#"
version = 1
mode = "client"
strict = true

[flags2env]
contract = ".cli-flags.toml"
require_audit = true
precedence = "argv-over-env"

[client]
enabled = true
api_base_url_binding = "api_base_url"
auth_token_binding = "auth_token"

[[env]]
name = "api_base_url"
key = "FANWAAVE_API_BASE_URL"
kind = "url"
required = true
secret = false
default = "https://default.example"

[[env]]
name = "auth_token"
key = "FANWAAVE_AUTH_TOKEN"
kind = "string"
required = true
secret = true
"#
}

#[test]
fn bundled_flags2env_supplies_only_argv_overrides() {
    let config = parse_fanwaave_config(client_config()).expect("valid config");
    let ambient = BTreeMap::from([
        (
            "FANWAAVE_API_BASE_URL".to_owned(),
            "https://ambient.example".to_owned(),
        ),
        (
            "FANWAAVE_AUTH_TOKEN".to_owned(),
            "test-only-secret".to_owned(),
        ),
    ]);
    let argv = vec![
        "fanwaave".to_owned(),
        "--api-base-url=https://argv.example".to_owned(),
    ];

    let resolved = resolve_fanwaave_config_from_argv(&config, &ambient, &argv)
        .expect("bundled parser should resolve argv");
    let api = resolved.binding("api_base_url").expect("API binding");
    assert_eq!(api.source(), ValueSource::Argv);
    assert_eq!(
        api.value(),
        &ConfigValue::Url("https://argv.example".to_owned())
    );

    let token = resolved.binding("auth_token").expect("secret binding");
    assert_eq!(token.source(), ValueSource::Environment);
    assert!(token.is_secret());
    assert!(!format!("{token:?}").contains("test-only-secret"));
}

#[test]
fn explicit_contract_root_is_independent_of_process_working_directory() {
    let config = parse_fanwaave_config(client_config()).expect("valid config");
    let root = tempfile::tempdir().expect("temporary contract root");
    std::fs::write(
        root.path().join(".cli-flags.toml"),
        r#"
[env]
files = []

[parse]
allow_unknown = false

[flags.api_base_url]
env = "FANWAAVE_API_BASE_URL"
aliases = ["api-base-url"]
type = "string"
"#,
    )
    .expect("write flags2env contract");

    let ambient = BTreeMap::from([(
        "FANWAAVE_AUTH_TOKEN".to_owned(),
        "test-only-secret".to_owned(),
    )]);
    let argv = vec![
        "fanwaave".to_owned(),
        "--api-base-url=https://argv.example".to_owned(),
    ];

    let resolved = resolve_fanwaave_config_from_argv_at(&config, &ambient, &argv, root.path())
        .expect("explicit contract root must be honored");
    let api = resolved.binding("api_base_url").expect("API binding");
    assert_eq!(api.source(), ValueSource::Argv);
    assert_eq!(
        api.value(),
        &ConfigValue::Url("https://argv.example".to_owned())
    );
    let token = resolved.binding("auth_token").expect("secret binding");
    assert_eq!(token.source(), ValueSource::Environment);
    assert!(token.is_secret());
}

#[test]
fn missing_explicit_contract_root_fails_closed_at_audit() {
    let config = parse_fanwaave_config(client_config()).expect("valid config");
    let root = tempfile::tempdir().expect("temporary contract root");
    let argv = vec!["fanwaave".to_owned()];

    let error = resolve_fanwaave_config_from_argv_at(
        &config,
        &BTreeMap::from([(
            "FANWAAVE_AUTH_TOKEN".to_owned(),
            "test-only-secret".to_owned(),
        )]),
        &argv,
        root.path(),
    )
    .expect_err("missing explicit flags2env contract must fail closed");
    assert!(matches!(error, FanwaaveFlags2EnvError::Audit));
}

#[test]
fn unknown_cli_options_fail_closed_without_echoing_argv() {
    let config = parse_fanwaave_config(client_config()).expect("valid config");
    let ambient = BTreeMap::from([(
        "FANWAAVE_AUTH_TOKEN".to_owned(),
        "test-only-secret".to_owned(),
    )]);
    let argv = vec![
        "fanwaave".to_owned(),
        "--not-a-real-fanwaave-option=secret-looking-value".to_owned(),
    ];

    let error = resolve_fanwaave_config_from_argv(&config, &ambient, &argv)
        .expect_err("unknown option must fail closed");
    assert!(matches!(error, FanwaaveFlags2EnvError::Arguments));
    let rendered = error.to_string();
    assert!(!rendered.contains("secret-looking-value"));
}
