//! Bootstrap relay management.
//!
//! Provides initial relays for DVM startup before config is loaded.

use nostr_sdk::Url;

/// Default bootstrap relays used when no configuration is available
pub const DEFAULT_BOOTSTRAP_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://relay.nostr.band",
];

/// Default admin app URL for pairing links
pub const DEFAULT_ADMIN_APP_URL: &str = "https://dvm-admin.example.com";

/// Returns bootstrap relays from environment or defaults.
///
/// Checks `BOOTSTRAP_RELAYS` environment variable first (comma-separated).
/// Falls back to hardcoded defaults.
pub fn get_bootstrap_relays() -> Vec<Url> {
    if let Ok(relays_str) = std::env::var("BOOTSTRAP_RELAYS") {
        relays_str
            .split(',')
            .filter_map(|s| Url::parse(s.trim()).ok())
            .collect()
    } else {
        DEFAULT_BOOTSTRAP_RELAYS
            .iter()
            .filter_map(|s| Url::parse(s).ok())
            .collect()
    }
}

/// Returns the admin app URL for pairing links.
///
/// Checks `DVM_ADMIN_APP_URL` environment variable first.
/// Falls back to default.
pub fn get_admin_app_url() -> String {
    std::env::var("DVM_ADMIN_APP_URL").unwrap_or_else(|_| DEFAULT_ADMIN_APP_URL.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_bootstrap_relays() {
        std::env::remove_var("BOOTSTRAP_RELAYS");

        let relays = get_bootstrap_relays();

        assert_eq!(relays.len(), 3);
        assert!(relays[0].to_string().contains("damus.io"));
    }

    #[test]
    fn test_custom_bootstrap_relays() {
        std::env::set_var("BOOTSTRAP_RELAYS", "wss://custom1.com,wss://custom2.com");

        let relays = get_bootstrap_relays();

        assert_eq!(relays.len(), 2);
        assert!(relays[0].to_string().contains("custom1.com"));

        std::env::remove_var("BOOTSTRAP_RELAYS");
    }

    #[test]
    fn test_admin_app_url_default() {
        std::env::remove_var("DVM_ADMIN_APP_URL");

        let url = get_admin_app_url();

        assert_eq!(url, DEFAULT_ADMIN_APP_URL);
    }

    #[test]
    fn test_admin_app_url_custom() {
        std::env::set_var("DVM_ADMIN_APP_URL", "https://my-admin.com");

        let url = get_admin_app_url();

        assert_eq!(url, "https://my-admin.com");

        std::env::remove_var("DVM_ADMIN_APP_URL");
    }
}
