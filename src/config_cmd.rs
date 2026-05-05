//! `nostube-transcode config` subcommands — remote config management via NIP-78.

use anyhow::{Context, Result};
use nostr_sdk::prelude::*;
use std::time::Duration;

use crate::bootstrap::get_bootstrap_relays;
use crate::identity::load_or_generate_identity;
use crate::paths::Paths;
use crate::remote_config::{self, RemoteConfig};

/// Connect to relays and return (client, keys, config).
/// Config is None if none exists yet on the relays.
async fn connect_and_fetch(paths: &Paths) -> Result<(Client, Keys, Option<RemoteConfig>)> {
    // Load env file so OPERATOR_NPUB is available if needed
    if paths.env_file.exists() {
        if let Ok(map) = crate::setup::read_env_file(&paths.env_file) {
            for (k, v) in &map {
                if std::env::var(k).is_err() {
                    unsafe { std::env::set_var(k, v) };
                }
            }
        }
    }

    let keys = load_or_generate_identity()
        .context("Failed to load DVM identity key — run: nostube-transcode setup")?;

    let relays = get_bootstrap_relays();
    let client = Client::new(keys.clone());
    for relay in &relays {
        client.add_relay(relay.as_str()).await.ok();
    }
    client.connect().await;

    // Brief wait for relay connections
    tokio::time::sleep(Duration::from_millis(500)).await;

    let config = remote_config::fetch_config(&client, &keys)
        .await
        .map_err(|e| match e {
            remote_config::RemoteConfigError::NotFound => None::<RemoteConfig>,
            _ => {
                eprintln!("Warning: could not fetch config: {e}");
                None
            }
        })
        .ok()
        .flatten();

    Ok((client, keys, config))
}

/// `config get` — display the current remote config as a table.
pub async fn get(paths: &Paths) -> Result<()> {
    let (_client, keys, config) = connect_and_fetch(paths).await?;

    let pubkey = keys.public_key();
    println!("DVM pubkey:    {}", pubkey.to_bech32().unwrap_or_else(|_| pubkey.to_hex()));
    println!();

    match config {
        None => {
            println!("No remote config found. Run `nostube-transcode setup` first.");
        }
        Some(cfg) => {
            println!("Paused:               {}", cfg.paused);
            println!("Max concurrent jobs:  {}", cfg.max_concurrent_jobs);
            println!("Blob expiration days: {}", cfg.blob_expiration_days);
            if let Some(ref name) = cfg.name {
                println!("Name:                 {name}");
            }
            if let Some(ref about) = cfg.about {
                println!("About:                {about}");
            }
            println!();
            println!("Relays ({}):", cfg.relays.len());
            for r in &cfg.relays {
                println!("  {r}");
            }
            println!();
            println!("Blossom servers ({}):", cfg.blossom_servers.len());
            for s in &cfg.blossom_servers {
                println!("  {s}");
            }
        }
    }

    Ok(())
}

/// `config set` — update one or more fields in the remote config.
#[allow(clippy::too_many_arguments)]
pub async fn set(
    paths: &Paths,
    relays: Option<Vec<String>>,
    blossom_servers: Option<Vec<String>>,
    max_concurrent_jobs: Option<u32>,
    blob_expiration_days: Option<u32>,
    name: Option<String>,
    about: Option<String>,
) -> Result<()> {
    let (client, keys, existing) = connect_and_fetch(paths).await?;
    let mut cfg = existing.unwrap_or_default();

    let mut changed = false;

    if let Some(r) = relays {
        cfg.relays = r;
        changed = true;
    }
    if let Some(b) = blossom_servers {
        cfg.blossom_servers = b;
        changed = true;
    }
    if let Some(n) = max_concurrent_jobs {
        cfg.max_concurrent_jobs = n;
        changed = true;
    }
    if let Some(d) = blob_expiration_days {
        cfg.blob_expiration_days = d;
        changed = true;
    }
    if let Some(n) = name {
        cfg.name = Some(n);
        changed = true;
    }
    if let Some(a) = about {
        cfg.about = Some(a);
        changed = true;
    }

    if !changed {
        println!("No changes specified. Use --relays, --blossom-servers, --max-concurrent-jobs, etc.");
        return Ok(());
    }

    remote_config::save_config(&client, &keys, &cfg)
        .await
        .context("Failed to save config to relays")?;

    println!("Config updated successfully.");
    Ok(())
}

/// `config pause` — set paused = true in remote config.
pub async fn pause(paths: &Paths) -> Result<()> {
    let (client, keys, existing) = connect_and_fetch(paths).await?;
    let mut cfg = existing.unwrap_or_default();
    if cfg.paused {
        println!("DVM is already paused.");
        return Ok(());
    }
    cfg.paused = true;
    remote_config::save_config(&client, &keys, &cfg)
        .await
        .context("Failed to save config")?;
    println!("DVM paused — it will stop accepting new jobs after restart.");
    Ok(())
}

/// `config resume` — set paused = false in remote config.
pub async fn resume(paths: &Paths) -> Result<()> {
    let (client, keys, existing) = connect_and_fetch(paths).await?;
    let mut cfg = existing.unwrap_or_default();
    if !cfg.paused {
        println!("DVM is not paused.");
        return Ok(());
    }
    cfg.paused = false;
    remote_config::save_config(&client, &keys, &cfg)
        .await
        .context("Failed to save config")?;
    println!("DVM resumed — it will accept new jobs after restart.");
    Ok(())
}

/// `config status` — show DVM pubkey and pause state.
pub async fn status(paths: &Paths) -> Result<()> {
    let (_client, keys, config) = connect_and_fetch(paths).await?;
    let pubkey = keys.public_key();
    println!("DVM pubkey: {}", pubkey.to_bech32().unwrap_or_else(|_| pubkey.to_hex()));
    match config {
        None => println!("Status:     no remote config found"),
        Some(cfg) => {
            if cfg.paused {
                println!("Status:     paused");
            } else {
                println!("Status:     active");
            }
        }
    }
    Ok(())
}
