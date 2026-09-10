use fanwaave_lib_core::fanwaave_config::{
    parse_fanwaave_config, resolve_fanwaave_config, ConfigValue, FanwaaveConfigError, ValueSource,
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
fn argv_overrides_environment_and_secret_debug_is_redacted() {
    let config = parse_fanwaave_config(client_config()).expect("config should parse");
    let ambient = BTreeMap::from([
        (
            "FANWAAVE_API_BASE_URL".to_owned(),
            "https://ambient.example".to_owned(),
        ),
        (
            "FANWAAVE_AUTH_TOKEN".to_owned(),
            "test-only-token-value".to_owned(),
        ),
    ]);
    let argv = BTreeMap::from([(
        "FANWAAVE_API_BASE_URL".to_owned(),
        "https://argv.example".to_owned(),
    )]);

    let resolved = resolve_fanwaave_config(&config, &ambient, &argv).expect("config resolves");
    let api = resolved.binding("api_base_url").expect("API binding");
    assert_eq!(api.source(), ValueSource::Argv);
    assert_eq!(
        api.value(),
        &ConfigValue::Url("https://argv.example".to_owned())
    );

    let token = resolved.binding("auth_token").expect("token binding");
    assert_eq!(token.source(), ValueSource::Environment);
    assert!(token.is_secret());
    let rendered = format!("{token:?}");
    assert!(rendered.contains("[REDACTED]"));
    assert!(!rendered.contains("test-only-token-value"));
}

#[test]
fn rejects_unknown_toml_fields() {
    let input = client_config().replace(
        "secret = true\n",
        "secret = true\nunknown_policy = \"must-not-be-ignored\"\n",
    );
    assert!(matches!(
        parse_fanwaave_config(&input),
        Err(FanwaaveConfigError::Toml(_))
    ));
}

#[test]
fn rejects_disabled_strict_or_flags_audit() {
    let strict_off = client_config().replace("strict = true", "strict = false");
    assert_eq!(
        parse_fanwaave_config(&strict_off).err(),
        Some(FanwaaveConfigError::StrictModeRequired)
    );

    let audit_off = client_config().replace("require_audit = true", "require_audit = false");
    assert_eq!(
        parse_fanwaave_config(&audit_off).err(),
        Some(FanwaaveConfigError::FlagsAuditRequired)
    );
}

#[test]
fn rejects_noncanonical_flags_contract() {
    let input = client_config().replace(
        "contract = \".cli-flags.toml\"",
        "contract = \"../shared/.cli-flags.toml\"",
    );
    assert_eq!(
        parse_fanwaave_config(&input).err(),
        Some(FanwaaveConfigError::UnsafeFlagsContract)
    );
}

#[test]
fn rejects_secret_defaults_and_secret_argv_values() {
    let with_default = client_config().replace(
        "key = \"FANWAAVE_AUTH_TOKEN\"\nkind = \"string\"\nrequired = true\nsecret = true",
        "key = \"FANWAAVE_AUTH_TOKEN\"\nkind = \"string\"\nrequired = true\nsecret = true\ndefault = \"not-allowed\"",
    );
    assert_eq!(
        parse_fanwaave_config(&with_default).err(),
        Some(FanwaaveConfigError::SecretDefault("auth_token".to_owned()))
    );

    let config = parse_fanwaave_config(client_config()).expect("config should parse");
    let ambient = BTreeMap::from([(
        "FANWAAVE_API_BASE_URL".to_owned(),
        "https://ambient.example".to_owned(),
    )]);
    let argv = BTreeMap::from([(
        "FANWAAVE_AUTH_TOKEN".to_owned(),
        "test-only-token-value".to_owned(),
    )]);
    assert_eq!(
        resolve_fanwaave_config(&config, &ambient, &argv).err(),
        Some(FanwaaveConfigError::SecretFromArgv("auth_token".to_owned()))
    );
}

#[test]
fn rejects_duplicate_binding_names_and_environment_keys() {
    let duplicate_name = format!(
        "{}\n[[env]]\nname = \"api_base_url\"\nkey = \"SECOND_API_URL\"\nkind = \"url\"\nrequired = false\nsecret = false\n",
        client_config()
    );
    assert_eq!(
        parse_fanwaave_config(&duplicate_name).err(),
        Some(FanwaaveConfigError::DuplicateBindingName(
            "api_base_url".to_owned()
        ))
    );

    let duplicate_key = format!(
        "{}\n[[env]]\nname = \"second_api\"\nkey = \"FANWAAVE_API_BASE_URL\"\nkind = \"url\"\nrequired = false\nsecret = false\n",
        client_config()
    );
    assert_eq!(
        parse_fanwaave_config(&duplicate_key).err(),
        Some(FanwaaveConfigError::DuplicateEnvironmentKey(
            "FANWAAVE_API_BASE_URL".to_owned()
        ))
    );
}

#[test]
fn rejects_role_mismatch_and_unknown_binding_reference() {
    let hybrid_without_server = client_config().replace("mode = \"client\"", "mode = \"hybrid\"");
    assert!(matches!(
        parse_fanwaave_config(&hybrid_without_server),
        Err(FanwaaveConfigError::ModeRoleMismatch(_))
    ));

    let unknown = client_config().replace(
        "api_base_url_binding = \"api_base_url\"",
        "api_base_url_binding = \"missing_api\"",
    );
    assert!(matches!(
        parse_fanwaave_config(&unknown),
        Err(FanwaaveConfigError::UnknownBindingReference { .. })
    ));
}

#[test]
fn requires_sensitive_references_to_point_at_secret_bindings() {
    let unsafe_token = client_config().replace(
        "key = \"FANWAAVE_AUTH_TOKEN\"\nkind = \"string\"\nrequired = true\nsecret = true",
        "key = \"FANWAAVE_AUTH_TOKEN\"\nkind = \"string\"\nrequired = true\nsecret = false",
    );
    assert!(matches!(
        parse_fanwaave_config(&unsafe_token),
        Err(FanwaaveConfigError::SensitiveBindingMustBeSecret { .. })
    ));
}

#[test]
fn rejects_missing_required_binding_without_default() {
    let config = parse_fanwaave_config(client_config()).expect("config should parse");
    let ambient = BTreeMap::new();
    let argv = BTreeMap::new();
    assert_eq!(
        resolve_fanwaave_config(&config, &ambient, &argv).err(),
        Some(FanwaaveConfigError::MissingRequiredBinding(
            "auth_token".to_owned()
        ))
    );
}

#[test]
fn coerces_supported_environment_kinds() {
    let input = r#"
version = 1
mode = "client"
strict = true

[flags2env]
contract = ".cli-flags.toml"
require_audit = true
precedence = "argv-over-env"

[client]
enabled = true

[[env]]
name = "switch"
key = "TEST_SWITCH"
kind = "bool"
required = true
secret = false

[[env]]
name = "count"
key = "TEST_COUNT"
kind = "integer"
required = true
secret = false

[[env]]
name = "ratio"
key = "TEST_RATIO"
kind = "double"
required = true
secret = false

[[env]]
name = "payload"
key = "TEST_PAYLOAD"
kind = "json"
required = true
secret = false

[[env]]
name = "endpoint"
key = "TEST_ENDPOINT"
kind = "url"
required = true
secret = false
"#;
    let config = parse_fanwaave_config(input).expect("config should parse");
    let ambient = BTreeMap::from([
        ("TEST_SWITCH".to_owned(), "on".to_owned()),
        ("TEST_COUNT".to_owned(), "42".to_owned()),
        ("TEST_RATIO".to_owned(), "0.25".to_owned()),
        ("TEST_PAYLOAD".to_owned(), "{\"ok\":true}".to_owned()),
        ("TEST_ENDPOINT".to_owned(), "https://example.test/v1".to_owned()),
    ]);
    let resolved =
        resolve_fanwaave_config(&config, &ambient, &BTreeMap::new()).expect("config resolves");

    assert_eq!(resolved.binding("switch").unwrap().value(), &ConfigValue::Bool(true));
    assert_eq!(resolved.binding("count").unwrap().value(), &ConfigValue::Integer(42));
    assert_eq!(resolved.binding("ratio").unwrap().value(), &ConfigValue::Double(0.25));
    assert_eq!(
        resolved.binding("endpoint").unwrap().value(),
        &ConfigValue::Url("https://example.test/v1".to_owned())
    );
    assert!(matches!(
        resolved.binding("payload").unwrap().value(),
        ConfigValue::Json(value) if value["ok"] == true
    ));
}

#[test]
fn invalid_scalar_values_fail_closed_without_echoing_values() {
    let input = r#"
version = 1
mode = "client"
strict = true

[flags2env]
contract = ".cli-flags.toml"
require_audit = true
precedence = "argv-over-env"

[client]
enabled = true

[[env]]
name = "endpoint"
key = "TEST_ENDPOINT"
kind = "url"
required = true
secret = false
"#;
    let config = parse_fanwaave_config(input).expect("config should parse");
    let ambient = BTreeMap::from([("TEST_ENDPOINT".to_owned(), "not a url".to_owned())]);
    let error = resolve_fanwaave_config(&config, &ambient, &BTreeMap::new())
        .expect_err("invalid URL must fail");
    assert_eq!(error, FanwaaveConfigError::InvalidUrl("endpoint".to_owned()));
    assert!(!error.to_string().contains("not a url"));
}
