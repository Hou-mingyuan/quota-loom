use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum DataSourceKind {
    ClaudeCode,
    CodexCli,
    ChatGptCodex,
}

impl DataSourceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::ClaudeCode => "Claude Code",
            Self::CodexCli => "Codex CLI",
            Self::ChatGptCodex => "ChatGPT Codex",
        }
    }

    pub fn brand(self) -> &'static str {
        match self {
            Self::ClaudeCode => "CLAUDE CODE / USAGE",
            Self::CodexCli => "CODEX CLI / USAGE",
            Self::ChatGptCodex => "CHATGPT CODEX / USAGE",
        }
    }

    pub fn window_title(self) -> &'static str {
        match self {
            Self::ClaudeCode => "QuotaLoom · Claude Code",
            Self::CodexCli => "QuotaLoom · Codex CLI",
            Self::ChatGptCodex => "QuotaLoom · ChatGPT Codex",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenTotals {
    pub input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
}

impl TokenTotals {
    pub fn is_zero(self) -> bool {
        self.input_tokens == 0 && self.cached_input_tokens == 0 && self.output_tokens == 0
    }

    pub fn fresh_input_tokens(self) -> u64 {
        self.input_tokens.saturating_sub(self.cached_input_tokens)
    }

    pub fn total_tokens(self) -> u64 {
        self.input_tokens.saturating_add(self.output_tokens)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UsageEvent {
    pub id: String,
    pub source_key: String,
    pub thread_id: String,
    pub event_index: u64,
    pub occurred_at: i64,
    pub model: String,
    pub tokens: TokenTotals,
    pub source_file: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionCursor {
    pub source_key: String,
    pub path: String,
    pub file_size: u64,
    pub modified_ns: u64,
    pub byte_offset: u64,
    pub event_index: u64,
    pub thread_id: Option<String>,
    pub current_model: String,
    pub previous_total: Option<TokenTotals>,
    pub replay_active: bool,
}

#[derive(Clone, Debug)]
pub struct ParseOutcome {
    pub cursor: SessionCursor,
    pub events: Vec<UsageEvent>,
    pub reset_required: bool,
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageRange {
    pub start_at: i64,
    pub end_at: i64,
    #[serde(default = "default_bucket_seconds")]
    pub bucket_seconds: i64,
    #[serde(default)]
    pub timezone_offset_seconds: i64,
}

fn default_bucket_seconds() -> i64 {
    3600
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSummary {
    pub total_tokens: u64,
    pub fresh_input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub calls: u64,
    pub threads: u64,
    pub cache_hit_rate: f64,
    pub estimated_cost_usd: Option<String>,
    pub unpriced_models: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageTrendPoint {
    pub bucket_start: i64,
    pub total_tokens: u64,
    pub fresh_input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub calls: u64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyCostPoint {
    pub day_start: i64,
    pub estimated_cost_usd: Option<String>,
    pub has_unpriced_usage: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelUsage {
    pub model: String,
    pub total_tokens: u64,
    pub fresh_input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub calls: u64,
    pub cache_hit_rate: f64,
    pub estimated_cost_usd: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelPriceEntry {
    pub model: String,
    pub input_per_million: String,
    pub cached_input_per_million: String,
    pub output_per_million: String,
    pub multiplier: String,
    pub configured: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentUsageEvent {
    pub id: String,
    pub thread_id: String,
    pub occurred_at: i64,
    pub model: String,
    pub total_tokens: u64,
    pub fresh_input_tokens: u64,
    pub cached_input_tokens: u64,
    pub output_tokens: u64,
    pub estimated_cost_usd: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageSnapshot {
    pub generated_at: i64,
    pub codex_home: String,
    pub source_kind: DataSourceKind,
    pub source_label: String,
    pub source_brand: String,
    pub summary: UsageSummary,
    pub trends: Vec<UsageTrendPoint>,
    pub daily_costs: Vec<DailyCostPoint>,
    pub models: Vec<ModelUsage>,
    pub recent: Vec<RecentUsageEvent>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WeeklyUsage {
    pub used_percent: f64,
    pub remaining_percent: f64,
    pub resets_at: Option<i64>,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub scanned_files: u64,
    pub changed_files: u64,
    pub imported_events: u64,
    pub rebuilt_files: u64,
    pub warnings: Vec<String>,
}
