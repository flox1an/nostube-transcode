//! Nostr Video Processing DVM
//!
//! A Data Vending Machine (DVM) that transforms videos into HLS format
//! and uploads them to Blossom servers.

pub mod blossom;
pub mod config;
pub mod dvm;
pub mod error;
pub mod nostr;
pub mod util;
pub mod video;
pub mod web;

pub use config::Config;
pub use error::{BlossomError, ConfigError, DvmError, VideoError};
