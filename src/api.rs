//! MiniMax API client - direct HTTP calls instead of CLI parsing

use anyhow::{Context, Result};
use serde::Deserialize;

const GLOBAL_ENDPOINT: &str = "https://api.minimax.io/v1/token_plan/remains";
const CN_ENDPOINT: &str = "https://api.minimaxi.com/v1/token_plan/remains";

#[derive(Debug, Clone, Deserialize)]
pub struct QuotaResponse {
    pub model_remains: Vec<ModelQuota>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModelQuota {
    pub model_name: String,
    pub start_time: u64,
    pub end_time: u64,
    pub remains_time: u64,
    pub current_interval_total_count: u32,
    pub current_interval_usage_count: u32,
    pub current_weekly_total_count: u32,
    pub current_weekly_usage_count: u32,
    pub weekly_start_time: u64,
    pub weekly_end_time: u64,
    pub weekly_remains_time: u64,
}

/// Aggregated usage data for display
#[derive(Debug, Clone)]
pub struct QuotaData {
    pub model: String,
    pub interval_used: u32,
    pub interval_max: u32,
    pub interval_remaining_ms: u64,
    pub weekly_used: u32,
    pub weekly_max: u32,
    pub weekly_remaining_ms: u64,
}

impl QuotaData {
    pub fn interval_percentage(&self) -> u32 {
        if self.interval_max == 0 {
            0
        } else {
            (self.interval_used as f64 / self.interval_max as f64 * 100.0) as u32
        }
    }

    pub fn weekly_percentage(&self) -> u32 {
        if self.weekly_max == 0 {
            0
        } else {
            (self.weekly_used as f64 / self.weekly_max as f64 * 100.0) as u32
        }
    }

    pub fn interval_reset_mins(&self) -> u32 {
        (self.interval_remaining_ms / 1000 / 60) as u32
    }

    pub fn weekly_reset_mins(&self) -> u32 {
        (self.weekly_remaining_ms / 1000 / 60) as u32
    }
}

pub struct ApiClient {
    api_key: String,
    region: ApiRegion,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ApiRegion {
    Global,
    CN,
}

impl ApiClient {
    pub fn new(api_key: String, region: ApiRegion) -> Self {
        Self { api_key, region }
    }

    fn endpoint(&self) -> &str {
        match self.region {
            ApiRegion::Global => GLOBAL_ENDPOINT,
            ApiRegion::CN => CN_ENDPOINT,
        }
    }

    pub fn fetch_quota(&self) -> Result<QuotaData> {
        let response = ureq::get(self.endpoint())
            .set("Authorization", &format!("Bearer {}", self.api_key))
            .set("Content-Type", "application/json")
            .call()
            .context("Failed to call MiniMax quota API")?;

        let quota_response: QuotaResponse = serde_json::from_str(
            &response.into_string().context("Failed to read response body")?
        ).context("Failed to parse quota response")?;

        self.parse_quota_response(quota_response)
    }

    pub fn fetch_quota_with_region_detection(&self) -> Result<QuotaData> {
        // Try global first
        let client_global = ApiClient::new(self.api_key.clone(), ApiRegion::Global);
        if let Ok(data) = client_global.fetch_quota() {
            return Ok(data);
        }

        // Fallback to CN
        let client_cn = ApiClient::new(self.api_key.clone(), ApiRegion::CN);
        client_cn.fetch_quota()
    }

    fn parse_quota_response(&self, response: QuotaResponse) -> Result<QuotaData> {
        let model = response.model_remains.iter()
            .find(|m| m.model_name == "MiniMax-M2.7")
            .or_else(|| response.model_remains.iter().find(|m| m.model_name.starts_with("MiniMax-M2")))
            .or_else(|| response.model_remains.iter().find(|m| m.current_interval_usage_count > 0))
            .or_else(|| response.model_remains.first())
            .context("No model quotas found in response")?;

        // current_interval_usage_count is the actual USED count
        let interval_used = model.current_interval_usage_count;

        let weekly_used = model.current_weekly_usage_count;

        Ok(QuotaData {
            model: model.model_name.clone(),
            interval_used,
            interval_max: model.current_interval_total_count,
            interval_remaining_ms: model.remains_time,
            weekly_used,
            weekly_max: model.current_weekly_total_count,
            weekly_remaining_ms: model.weekly_remains_time,
        })
    }
}

pub fn fetch_blocking(api_key: &str) -> Result<QuotaData> {
    let client = ApiClient::new(api_key.to_string(), ApiRegion::Global);
    let data = client.fetch_quota_with_region_detection()?;
    eprintln!("DEBUG API raw: interval_used={}, interval_max={}, weekly_used={}, weekly_max={}",
        data.interval_used, data.interval_max, data.weekly_used, data.weekly_max);
    Ok(data)
}