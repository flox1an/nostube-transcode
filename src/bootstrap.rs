//! Bootstrap relay management.
//!
//! Provides initial relays for DVM startup before config is loaded.

use nostr_sdk::Url;

/// Default bootstrap relays used when no configuration is available
pub const DEFAULT_BOOTSTRAP_RELAYS: &[&str] = &[
    "wss://relay.damus.io",
    "wss://nos.lol",
    "wss://haven.slidestr.net",
];

/// Default HTTP port for the local web server
const DEFAULT_HTTP_PORT: u16 = 3000;

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
/// Falls back to local server URL based on `HTTP_PORT`.
pub fn get_admin_app_url() -> String {
    if let Ok(url) = std::env::var("DVM_ADMIN_APP_URL") {
        return url;
    }

    let port: u16 = std::env::var("HTTP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_HTTP_PORT);

    format!("http://localhost:{}", port)
}

/// Returns all admin app URLs for pairing links.
///
/// If `DVM_ADMIN_APP_URL` is set, returns only that URL.
/// Otherwise, returns URLs for all local network interfaces.
pub fn get_admin_app_urls() -> Vec<String> {
    if let Ok(url) = std::env::var("DVM_ADMIN_APP_URL") {
        return vec![url];
    }

    let port: u16 = std::env::var("HTTP_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(DEFAULT_HTTP_PORT);

    let mut urls = vec![format!("http://localhost:{}", port)];

    // Add URLs for all network interfaces
    if let Ok(interfaces) = get_if_addrs::get_if_addrs() {
        for iface in interfaces {
            let ip = iface.ip();
            // Skip loopback addresses (already covered by localhost)
            if ip.is_loopback() {
                continue;
            }
            // Format IPv6 addresses with brackets
            let addr = if ip.is_ipv6() {
                format!("http://[{}]:{}", ip, port)
            } else {
                format!("http://{}:{}", ip, port)
            };
            urls.push(addr);
        }
    }

    urls
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

        assert_eq!(relays.len(), 3);
        assert!(relays[0].to_string().contains("damus.io"));
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

    #[test]
    fn test_admin_app_url_default() {
        let _lock = ENV_MUTEX.lock().unwrap();
        std::env::remove_var("DVM_ADMIN_APP_URL");
        std::env::remove_var("HTTP_PORT");

        let url = get_admin_app_url();

        assert_eq!(url, "http://localhost:3000");
    }

    #[test]
    fn test_admin_app_url_custom_port() {
        let _lock = ENV_MUTEX.lock().unwrap();
        std::env::remove_var("DVM_ADMIN_APP_URL");
        std::env::set_var("HTTP_PORT", "8080");

        let url = get_admin_app_url();

        assert_eq!(url, "http://localhost:8080");

        std::env::remove_var("HTTP_PORT");
    }

    #[test]
    fn test_admin_app_url_custom() {
        let _lock = ENV_MUTEX.lock().unwrap();
        std::env::set_var("DVM_ADMIN_APP_URL", "https://my-admin.com");

        let url = get_admin_app_url();

        assert_eq!(url, "https://my-admin.com");

        std::env::remove_var("DVM_ADMIN_APP_URL");
    }
}
