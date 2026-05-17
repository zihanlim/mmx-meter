//! Quota data structures and parsing
//! Note: This module re-exports from api.rs for backwards compatibility
//! The actual fetching is done via api.rs directly

pub use crate::api::{QuotaData, QuotaResponse, ModelQuota};

/// Parse quota data from API response - re-exported for compatibility
pub fn parse_quota_data(response: &QuotaResponse) -> Option<QuotaData> {
    let model = response.model_remains.iter()
        .find(|m| m.model_name == "MiniMax-M2.7")
        .or_else(|| response.model_remains.iter().find(|m| m.model_name.starts_with("MiniMax-M2")))
        .or_else(|| response.model_remains.iter().find(|m| m.current_interval_usage_count > 0))
        .or_else(|| response.model_remains.first())?;

    // BUG FIX: current_interval_usage_count is actually REMAINING
    let interval_remaining = model.current_interval_usage_count;
    let interval_used = model.current_interval_total_count.saturating_sub(interval_remaining);

    let weekly_remaining = model.current_weekly_usage_count;
    let weekly_used = model.current_weekly_total_count.saturating_sub(weekly_remaining);

    Some(QuotaData {
        model: model.model_name.clone(),
        interval_used,
        interval_max: model.current_interval_total_count,
        interval_remaining_ms: model.remains_time,
        weekly_used,
        weekly_max: model.current_weekly_total_count,
        weekly_remaining_ms: model.weekly_remains_time,
    })
}