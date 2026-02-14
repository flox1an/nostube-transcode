//! Integration tests for remote configuration.
//!
//! Tests the interaction between identity, pairing, config, and admin modules.

use dvm_video_processing::admin::commands::{
    parse_request, AdminCommand, AdminResponse, AdminResponseWire, ConfigData, ConfigResponse,
    ResponseData, StatusResponse,
};
use dvm_video_processing::bootstrap::{
    get_admin_app_url, get_bootstrap_relays, DEFAULT_BOOTSTRAP_RELAYS,
};
use dvm_video_processing::pairing::PairingState;
use dvm_video_processing::remote_config::RemoteConfig;
use nostr_sdk::Keys;

/// Test the full pairing flow: identity -> pairing state -> URL generation -> claim parsing
#[test]
fn test_full_pairing_flow() {
    // Step 1: Generate identity (in real code this would be load_or_generate_identity)
    let keys = Keys::generate();

    // Step 2: Create pairing state
    let pairing_state = PairingState::new(keys.public_key());

    // Step 3: Verify pairing state is valid
    assert!(pairing_state.is_valid());

    // Step 4: Generate pairing URL
    let url = pairing_state.pairing_url("https://admin.example.com");
    assert!(url.starts_with("https://admin.example.com/admin/pair?"));
    assert!(url.contains("dvm=npub1"));
    assert!(url.contains("secret="));

    // Step 5: Extract secret from URL (simulating what admin app would do)
    let secret = url
        .split("secret=")
        .nth(1)
        .expect("URL should contain secret");

    // Step 6: Verify correct secret works
    assert!(pairing_state.verify(secret));

    // Verify wrong secret fails
    assert!(!pairing_state.verify("wrong-secr-etxx"));

    // Step 7: Parse the claim command (simulating admin sending v2 request)
    let claim_json = format!(
        r#"{{"id":"req-1","method":"claim_admin","params":{{"secret":"{}"}}}}"#,
        secret
    );
    let req = parse_request(&claim_json).unwrap();
    let cmd = req.to_command().unwrap();

    match cmd {
        AdminCommand::ClaimAdmin { secret: s } => {
            assert_eq!(s, secret);
        }
        _ => panic!("Expected ClaimAdmin command"),
    }
}

/// Test config serialization roundtrip
#[test]
fn test_config_roundtrip() {
    let config = RemoteConfig {
        version: 1,
        admin: Some("npub1testpubkey".to_string()),
        relays: vec![
            "wss://relay.damus.io".to_string(),
            "wss://nos.lol".to_string(),
        ],
        blossom_servers: vec!["https://blossom.example.com".to_string()],
        blob_expiration_days: 45,
        name: Some("Test DVM".to_string()),
        about: Some("Integration test DVM".to_string()),
        paused: false,
    };

    // Serialize to JSON
    let json = serde_json::to_string(&config).unwrap();

    // Deserialize back
    let parsed: RemoteConfig = serde_json::from_str(&json).unwrap();

    // Verify all fields match
    assert_eq!(parsed.version, 1);
    assert_eq!(parsed.admin, Some("npub1testpubkey".to_string()));
    assert_eq!(parsed.relays.len(), 2);
    assert_eq!(parsed.relays[0], "wss://relay.damus.io");
    assert_eq!(parsed.relays[1], "wss://nos.lol");
    assert_eq!(parsed.blossom_servers.len(), 1);
    assert_eq!(parsed.blob_expiration_days, 45);
    assert_eq!(parsed.name, Some("Test DVM".to_string()));
    assert_eq!(parsed.about, Some("Integration test DVM".to_string()));
    assert!(!parsed.paused);

    // Test has_admin helper
    assert!(parsed.has_admin());

    // Test empty config
    let empty_config = RemoteConfig::new();
    assert!(!empty_config.has_admin());
    assert_eq!(empty_config.version, 1);
    assert_eq!(empty_config.blob_expiration_days, 30);
}

/// Test bootstrap relays defaults
#[test]
fn test_bootstrap_relays() {
    // Clear env var to test defaults
    std::env::remove_var("BOOTSTRAP_RELAYS");

    let relays = get_bootstrap_relays();

    // Should have default relays
    assert_eq!(relays.len(), DEFAULT_BOOTSTRAP_RELAYS.len());

    // Each relay should be a valid URL
    for relay in &relays {
        assert!(relay.scheme() == "wss" || relay.scheme() == "ws");
    }

    // Test admin app URL default (uses local HTTP server)
    std::env::remove_var("DVM_ADMIN_APP_URL");
    std::env::remove_var("HTTP_PORT");
    let admin_url = get_admin_app_url();
    assert_eq!(admin_url, "http://localhost:3000");
}

/// Test admin response serialization via v2 wire format
#[test]
fn test_admin_response_serialization() {
    // Test simple ok response
    let ok_wire = AdminResponseWire::from_response("req-1".to_string(), AdminResponse::ok());
    let ok_json = serde_json::to_string(&ok_wire).unwrap();
    let ok_parsed: serde_json::Value = serde_json::from_str(&ok_json).unwrap();
    assert_eq!(ok_parsed["id"], "req-1");
    assert_eq!(ok_parsed["result"], serde_json::json!({}));
    assert!(ok_parsed.get("error").is_none() || ok_parsed["error"].is_null());

    // Test ok with message
    let msg_wire = AdminResponseWire::from_response(
        "req-2".to_string(),
        AdminResponse::ok_with_msg("Configuration updated successfully"),
    );
    let msg_json = serde_json::to_string(&msg_wire).unwrap();
    let msg_parsed: serde_json::Value = serde_json::from_str(&msg_json).unwrap();
    assert_eq!(msg_parsed["id"], "req-2");
    assert_eq!(msg_parsed["result"]["msg"], "Configuration updated successfully");

    // Test error response
    let err_wire = AdminResponseWire::from_response(
        "req-3".to_string(),
        AdminResponse::error("Invalid pairing secret"),
    );
    let err_json = serde_json::to_string(&err_wire).unwrap();
    let err_parsed: serde_json::Value = serde_json::from_str(&err_json).unwrap();
    assert_eq!(err_parsed["id"], "req-3");
    assert_eq!(err_parsed["error"], "Invalid pairing secret");
    assert!(err_parsed.get("result").is_none() || err_parsed["result"].is_null());

    // Test config response with data
    let config_data = ConfigData {
        relays: vec!["wss://relay.example.com".to_string()],
        blossom_servers: vec!["https://blossom.example.com".to_string()],
        blob_expiration_days: 30,
        name: Some("My DVM".to_string()),
        about: None,
        paused: false,
    };
    let config_wire = AdminResponseWire::from_response(
        "req-4".to_string(),
        AdminResponse::ok_with_data(ResponseData::Config(ConfigResponse {
            config: config_data,
        })),
    );
    let config_json = serde_json::to_string(&config_wire).unwrap();
    let config_parsed: serde_json::Value = serde_json::from_str(&config_json).unwrap();

    assert_eq!(config_parsed["id"], "req-4");
    assert_eq!(config_parsed["result"]["config"]["relays"][0], "wss://relay.example.com");
    assert_eq!(config_parsed["result"]["config"]["blossom_servers"][0], "https://blossom.example.com");
    assert_eq!(config_parsed["result"]["config"]["blob_expiration_days"], 30);
    assert_eq!(config_parsed["result"]["config"]["paused"], false);

    // Test status response
    let status_wire = AdminResponseWire::from_response(
        "req-5".to_string(),
        AdminResponse::ok_with_data(ResponseData::Status(StatusResponse {
            paused: false,
            jobs_active: 2,
            jobs_completed: 15,
            jobs_failed: 1,
            uptime_secs: 3600,
            hwaccel: "videotoolbox".to_string(),
            version: "0.1.0".to_string(),
        })),
    );
    let status_json = serde_json::to_string(&status_wire).unwrap();
    let status_parsed: serde_json::Value = serde_json::from_str(&status_json).unwrap();

    assert_eq!(status_parsed["id"], "req-5");
    assert_eq!(status_parsed["result"]["paused"], false);
    assert_eq!(status_parsed["result"]["jobs_active"], 2);
    assert_eq!(status_parsed["result"]["jobs_completed"], 15);
    assert_eq!(status_parsed["result"]["jobs_failed"], 1);
    assert_eq!(status_parsed["result"]["uptime_secs"], 3600);
    assert_eq!(status_parsed["result"]["hwaccel"], "videotoolbox");
}

/// Test command parsing for all admin commands via v2 wire format
#[test]
fn test_admin_command_parsing() {
    // Helper to parse a v2 request and convert to command
    fn parse_cmd(method: &str, params: &str) -> AdminCommand {
        let json = format!(r#"{{"id":"test","method":"{}","params":{}}}"#, method, params);
        let req = parse_request(&json).unwrap();
        req.to_command().unwrap()
    }

    // GetConfig
    assert_eq!(parse_cmd("get_config", "{}"), AdminCommand::GetConfig);

    // SetRelays
    let set_relays = parse_cmd("set_relays", r#"{"relays":["wss://relay1.com","wss://relay2.com"]}"#);
    assert!(matches!(set_relays, AdminCommand::SetRelays { relays } if relays.len() == 2));

    // SetBlossomServers
    let set_blossom = parse_cmd("set_blossom_servers", r#"{"servers":["https://b1.com"]}"#);
    assert!(matches!(
        set_blossom,
        AdminCommand::SetBlossomServers { servers } if servers.len() == 1
    ));

    // SetBlobExpiration
    let set_expiration = parse_cmd("set_blob_expiration", r#"{"days":60}"#);
    assert!(matches!(
        set_expiration,
        AdminCommand::SetBlobExpiration { days: 60 }
    ));

    // SetProfile
    let set_profile = parse_cmd("set_profile", r#"{"name":"Test","about":"Description"}"#);
    assert!(matches!(
        set_profile,
        AdminCommand::SetProfile {
            name: Some(_),
            about: Some(_)
        }
    ));

    // Pause/Resume
    assert_eq!(parse_cmd("pause", "{}"), AdminCommand::Pause);
    assert_eq!(parse_cmd("resume", "{}"), AdminCommand::Resume);

    // Status
    assert_eq!(parse_cmd("status", "{}"), AdminCommand::Status);

    // JobHistory with default limit
    assert!(matches!(
        parse_cmd("job_history", "{}"),
        AdminCommand::JobHistory { limit: 20 }
    ));

    // JobHistory with custom limit
    assert!(matches!(
        parse_cmd("job_history", r#"{"limit":50}"#),
        AdminCommand::JobHistory { limit: 50 }
    ));

    // SelfTest
    assert_eq!(parse_cmd("self_test", "{}"), AdminCommand::SelfTest);

    // SystemInfo
    assert_eq!(parse_cmd("system_info", "{}"), AdminCommand::SystemInfo);

    // ImportEnvConfig
    assert_eq!(parse_cmd("import_env_config", "{}"), AdminCommand::ImportEnvConfig);
}

/// Test that config defaults work correctly when parsing minimal JSON
#[test]
fn test_config_default_values() {
    // Minimal config with just version
    let json = r#"{"version": 1}"#;
    let config: RemoteConfig = serde_json::from_str(json).unwrap();

    assert_eq!(config.version, 1);
    assert!(config.admin.is_none());
    assert!(config.relays.is_empty());
    assert!(config.blossom_servers.is_empty());
    assert_eq!(config.blob_expiration_days, 30); // default
    assert!(config.name.is_none());
    assert!(config.about.is_none());
    assert!(!config.paused); // default false
}
