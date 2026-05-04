//! Nostr Video Processing DVM
//!
//! A Data Vending Machine (DVM) that transforms videos into HLS format
//! and uploads them to Blossom servers.

pub mod admin;
pub mod blossom;
pub mod bootstrap;
pub mod cli;
pub mod config;
pub mod config_cmd;
pub mod docker_cmd;
pub mod doctor;
pub mod dvm;
pub mod dvm_state;
pub mod error;
pub mod identity;
pub mod nostr;
pub mod paths;
pub mod remote_config;
pub mod runtime;
pub mod selftest;
pub mod service;
pub mod setup;
pub mod startup;
pub mod update_cmd;
pub mod util;
pub mod video;
pub mod web;

pub use config::Config;
pub use error::{BlossomError, ConfigError, DvmError, VideoError};
