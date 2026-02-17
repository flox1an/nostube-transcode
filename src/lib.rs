//! Nostr Video Processing DVM
//!
//! A Data Vending Machine (DVM) that transforms videos into HLS format
//! and uploads them to Blossom servers.

pub mod admin;
pub mod blossom;
pub mod bootstrap;
pub mod config;
pub mod dvm;
pub mod dvm_state;
pub mod error;
pub mod identity;
pub mod nostr;
pub mod remote_config;
pub mod startup;
pub mod util;
pub mod video;
pub mod web;

pub use config::Config;
pub use error::{BlossomError, ConfigError, DvmError, VideoError};
