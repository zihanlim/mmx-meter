//! Credential discovery for MiniMax API
//! Tries: env var > CLI arg > ~/.claude/.credentials.json > config file

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Credentials {
    pub api_key: String,
    pub region: Region,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Region {
    Global,
    CN,
    Auto,
}

impl Default for Region {
    fn default() -> Self {
        Region::Auto
    }
}

/// Load credentials from environment variable
fn from_env() -> Option<String> {
    std::env::var("MINIMAX_API_KEY").ok().filter(|k| !k.is_empty())
}

/// Load credentials from ~/.claude/.credentials.json
fn from_claude_credentials() -> Option<String> {
    let path = dirs::home_dir()?.join(".claude").join(".credentials.json");
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    // Try to parse as JSON and extract minimax api key
    #[derive(Deserialize)]
    struct ClaudeCredentials {
        #[serde(rename = "minimax")]
        minimax_key: Option<String>,
        #[serde(rename = "apiKey")]
        api_key: Option<String>,
    }
    if let Ok(creds) = serde_json::from_str::<ClaudeCredentials>(&content) {
        creds.minimax_key.or(creds.api_key)
    } else {
        // Try raw key extraction
        let raw: serde_json::Value = serde_json::from_str(&content).ok()?;
        if let Some(key) = raw.get("minimax").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            Some(key.to_string())
        } else if let Some(key) = raw.get("apiKey").and_then(|v| v.as_str()).filter(|s| !s.is_empty()) {
            Some(key.to_string())
        } else {
            None
        }
    }
}

/// Load credentials from app config file
fn from_config_file() -> Option<String> {
    let path = config_path()?;
    if !path.exists() {
        return None;
    }
    let content = std::fs::read_to_string(&path).ok()?;
    #[derive(Deserialize)]
    struct Config {
        api_key: Option<String>,
    }
    serde_json::from_str::<Config>(&content)
        .ok()
        .and_then(|c| c.api_key)
        .filter(|k| !k.is_empty())
}

fn config_path() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("mmx-meter").join("config.json"))
}

pub fn load(api_key_arg: Option<&str>) -> Result<Credentials> {
    // Priority: CLI arg > env var > claude credentials > config file
    let api_key = api_key_arg
        .filter(|k| !k.is_empty())
        .map(String::from)
        .or_else(from_env)
        .or_else(from_claude_credentials)
        .or_else(from_config_file)
        .context("No MiniMax API key found. Set MINIMAX_API_KEY env var or pass --api-key <key>")?;

    Ok(Credentials {
        api_key,
        region: Region::Auto,
    })
}