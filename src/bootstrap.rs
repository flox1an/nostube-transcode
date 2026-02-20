//! Bootstrap relay management.
//!
//! Provides initial relays for DVM startup before config is loaded.

use nostr_sdk::Url;

/// Default bootstrap relays used when no configuration is available
pub const DEFAULT_BOOTSTRAP_RELAYS: &[&str] = &[
    "wss://nos.lol",
    "wss://relay.damus.io",
    "wss://relay.nostu.be",
    "wss://relay.snort.social",
];

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // Mutex to serialize tests that modify environment variables
    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_default_bootstrap_relays() {
        let _lock = ENV_MUTEX.lock().unwrap();
        std::env::remove_var("BOOTSTRAP_RELAYS");

        let relays = get_bootstrap_relays();

        assert_eq!(relays.len(), 4);
        assert!(relays[0].to_string().contains("nos.lol"));
    }

    #[test]
    fn test_custom_bootstrap_relays() {
        let _lock = ENV_MUTEX.lock().unwrap();
        std::env::set_var("BOOTSTRAP_RELAYS", "wss://custom1.com,wss://custom2.com");

        let relays = get_bootstrap_relays();

        assert_eq!(relays.len(), 2);
        assert!(relays[0].to_string().contains("custom1.com"));

        std::env::remove_var("BOOTSTRAP_RELAYS");
    }
}
