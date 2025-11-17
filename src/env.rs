use std::env;

#[derive(Debug, Clone)]
pub struct Config {
    /// DVM's Nostr private key (hex format)
    pub nostr_private_key: String,

    /// Comma-separated list of Nostr relay URLs
    pub nostr_relays: Vec<String>,

    /// Comma-separated list of Blossom upload server URLs
    pub blossom_upload_servers: Vec<String>,

    /// Number of days to keep blobs before cleanup (default: 30)
    pub blossom_blob_expiration_days: u64,

    /// LNbits URL for payment integration (optional)
    pub lnbits_url: Option<String>,

    /// LNbits admin key for payment integration (optional)
    pub lnbits_admin_key: Option<String>,
}

impl Config {
    pub fn from_env() -> Result<Self, String> {
        dotenv::dotenv().ok();

        let nostr_private_key = env::var("NOSTR_PRIVATE_KEY")
            .map_err(|_| "NOSTR_PRIVATE_KEY environment variable is required".to_string())?;

        let nostr_relays_str = env::var("NOSTR_RELAYS")
            .map_err(|_| "NOSTR_RELAYS environment variable is required".to_string())?;
        let nostr_relays: Vec<String> = nostr_relays_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if nostr_relays.is_empty() {
            return Err("NOSTR_RELAYS must contain at least one relay URL".to_string());
        }

        let blossom_servers_str = env::var("BLOSSOM_UPLOAD_SERVERS")
            .map_err(|_| "BLOSSOM_UPLOAD_SERVERS environment variable is required".to_string())?;
        let blossom_upload_servers: Vec<String> = blossom_servers_str
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        if blossom_upload_servers.is_empty() {
            return Err("BLOSSOM_UPLOAD_SERVERS must contain at least one server URL".to_string());
        }

        let blossom_blob_expiration_days = env::var("BLOSSOM_BLOB_EXPIRATION_DAYS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(30);

        let lnbits_url = env::var("LNBITS_URL").ok();
        let lnbits_admin_key = env::var("LNBITS_ADMIN_KEY").ok();

        Ok(Config {
            nostr_private_key,
            nostr_relays,
            blossom_upload_servers,
            blossom_blob_expiration_days,
            lnbits_url,
            lnbits_admin_key,
        })
    }
}
