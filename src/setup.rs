//! Interactive and non-interactive DVM setup wizard.

use anyhow::{bail, Context, Result};
use nostr_sdk::PublicKey;
use std::collections::BTreeMap;
use std::io::{self, BufRead, Write};
use std::path::Path;

/// Read key=value pairs from an env file. Lines starting with # are ignored.
pub fn read_env_file(path: &Path) -> Result<BTreeMap<String, String>> {
    let content = std::fs::read_to_string(path).context("Failed to read env file")?;
    let mut map = BTreeMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, val)) = line.split_once('=') {
            map.insert(key.trim().to_string(), val.trim().to_string());
        }
    }
    Ok(map)
}

/// Write key=value pairs to an env file (creates or overwrites, atomic).
pub fn write_env_file(path: &Path, entries: &[(&str, &str)]) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).context("Failed to create data directory")?;
    }
    let mut content = String::new();
    for (k, v) in entries {
        content.push_str(&format!("{k}={v}\n"));
    }
    atomic_write(path, content.as_bytes())
}

/// Update specific keys in an env file while preserving all other keys.
pub fn upsert_env_file(path: &Path, updates: &[(&str, &str)]) -> Result<()> {
    let mut map = if path.exists() {
        read_env_file(path).unwrap_or_default()
    } else {
        BTreeMap::new()
    };
    for (k, v) in updates {
        map.insert(k.to_string(), v.to_string());
    }
    let pairs: Vec<(String, String)> = map.into_iter().collect();
    let entries: Vec<(&str, &str)> = pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    write_env_file(path, &entries)
}

/// Validate that a string is a valid Nostr public key (npub or 64-char hex).
pub fn validate_operator_npub(s: &str) -> Result<PublicKey> {
    if s.is_empty() {
        bail!("OPERATOR_NPUB cannot be empty");
    }
    PublicKey::parse(s).map_err(|e| anyhow::anyhow!("Invalid OPERATOR_NPUB '{}': {}", s, e))
}

/// Run the interactive setup wizard.
///
/// `non_interactive`: skip prompts, use provided flags / existing env file values.
pub fn run_setup(
    paths: &crate::paths::Paths,
    non_interactive: bool,
    operator_npub: Option<&str>,
    http_port: Option<u16>,
) -> Result<()> {
    println!("nostube-transcode setup");
    println!("=======================");
    println!("Data dir: {}", paths.data_dir.display());
    println!("Env file: {}", paths.env_file.display());
    println!();

    // Load existing config
    let mut env = if paths.env_file.exists() {
        read_env_file(&paths.env_file).unwrap_or_default()
    } else {
        BTreeMap::new()
    };

    // OPERATOR_NPUB
    let npub = if let Some(n) = operator_npub {
        validate_operator_npub(n).context("--operator-npub is invalid")?;
        n.to_string()
    } else if let Some(existing) = env.get("OPERATOR_NPUB").cloned() {
        if validate_operator_npub(&existing).is_ok() {
            println!("OPERATOR_NPUB: {} (existing)", existing);
            existing
        } else if non_interactive {
            bail!("Existing OPERATOR_NPUB is invalid. Re-run with --operator-npub <npub>.");
        } else {
            prompt_operator_npub()
        }
    } else if non_interactive {
        bail!("OPERATOR_NPUB is required. Pass --operator-npub <npub>.");
    } else {
        prompt_operator_npub()
    };

    env.insert("OPERATOR_NPUB".to_string(), npub);

    // HTTP_PORT
    if let Some(port) = http_port {
        env.insert("HTTP_PORT".to_string(), port.to_string());
    }

    // Write env file
    std::fs::create_dir_all(&paths.data_dir)
        .context("Failed to create data directory")?;
    let pairs: Vec<(String, String)> = env.into_iter().collect();
    let entries: Vec<(&str, &str)> =
        pairs.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    write_env_file(&paths.env_file, &entries)?;

    // Set permissions to 0600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&paths.env_file)?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&paths.env_file, perms)?;
    }

    println!("Configuration written to {}", paths.env_file.display());

    // Offer service install (interactive only)
    if !non_interactive {
        print!("\nInstall and start the background service now? [Y/n] ");
        io::stdout().flush().ok();
        let mut line = String::new();
        io::stdin().lock().read_line(&mut line).ok();
        let answer = line.trim().to_lowercase();
        if answer.is_empty() || answer == "y" || answer == "yes" {
            crate::service::install_and_start(paths, false, false, None)?;
        }
    }

    let http_port_display = pairs
        .iter()
        .find(|(k, _)| k == "HTTP_PORT")
        .map(|(_, v)| v.as_str())
        .unwrap_or("5207");

    println!("\nSetup complete!");
    println!("Admin UI: http://localhost:{}", http_port_display);

    Ok(())
}

fn prompt_operator_npub() -> String {
    loop {
        print!("Enter your OPERATOR_NPUB (npub1... or 64-char hex): ");
        io::stdout().flush().ok();

        // Try /dev/tty for piped installs (curl | bash)
        let input = if let Ok(tty) = std::fs::File::open("/dev/tty") {
            let mut reader = io::BufReader::new(tty);
            let mut s = String::new();
            reader.read_line(&mut s).ok();
            s
        } else {
            let mut s = String::new();
            io::stdin().lock().read_line(&mut s).ok();
            s
        };

        let input = input.trim().to_string();
        if validate_operator_npub(&input).is_ok() {
            return input;
        }
        eprintln!("Invalid format. Must be npub1... or 64-char hex pubkey.");
    }
}

/// Write bytes to a path atomically (write to .tmp then rename).
fn atomic_write(path: &Path, data: &[u8]) -> Result<()> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).context("Failed to write temp env file")?;
    std::fs::rename(&tmp, path).context("Failed to rename env file into place")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_write_new_env_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("env");
        write_env_file(&path, &[("OPERATOR_NPUB", "npub1test"), ("HTTP_PORT", "5207")])
            .unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("OPERATOR_NPUB=npub1test"));
        assert!(content.contains("HTTP_PORT=5207"));
    }

    #[test]
    fn test_update_preserves_unknown_keys() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("env");
        std::fs::write(&path, "OPERATOR_NPUB=old\nCUSTOM_KEY=my_value\n").unwrap();
        upsert_env_file(&path, &[("OPERATOR_NPUB", "new")]).unwrap();
        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("OPERATOR_NPUB=new"));
        assert!(content.contains("CUSTOM_KEY=my_value"));
    }

    #[test]
    fn test_read_env_file() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("env");
        std::fs::write(&path, "OPERATOR_NPUB=npub1abc\nHTTP_PORT=9000\n").unwrap();
        let vals = read_env_file(&path).unwrap();
        assert_eq!(
            vals.get("OPERATOR_NPUB").map(|s| s.as_str()),
            Some("npub1abc")
        );
        assert_eq!(vals.get("HTTP_PORT").map(|s| s.as_str()), Some("9000"));
    }

    #[test]
    fn test_validate_operator_npub_valid_npub() {
        // secp256k1 generator point x-coord — always a valid public key
        let result = validate_operator_npub(
            "79be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798",
        );
        assert!(result.is_ok(), "expected ok, got: {:?}", result);
    }

    #[test]
    fn test_validate_operator_npub_invalid() {
        assert!(validate_operator_npub("notanpub").is_err());
        assert!(validate_operator_npub("").is_err());
        assert!(validate_operator_npub("npub1tooshort").is_err());
    }

    #[test]
    fn test_read_env_file_ignores_comments() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("env");
        std::fs::write(&path, "# comment\nOPERATOR_NPUB=abc\n\n").unwrap();
        let vals = read_env_file(&path).unwrap();
        assert_eq!(vals.len(), 1);
        assert!(vals.contains_key("OPERATOR_NPUB"));
    }
}
