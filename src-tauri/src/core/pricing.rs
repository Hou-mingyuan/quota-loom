use crate::core::types::TokenTotals;
use rust_decimal::Decimal;
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct CatalogModelPrice {
    pub model: String,
    pub display_name: String,
    pub input_per_million: String,
    pub cached_input_per_million: String,
    pub output_per_million: String,
}

#[derive(Clone, Debug)]
pub struct ModelPrice {
    pub input_per_million: Decimal,
    pub cached_input_per_million: Decimal,
    pub output_per_million: Decimal,
    pub multiplier: Decimal,
}

impl ModelPrice {
    pub fn from_strings(input: &str, cached: &str, output: &str, multiplier: &str) -> Option<Self> {
        Some(Self {
            input_per_million: Decimal::from_str(input).ok()?,
            cached_input_per_million: Decimal::from_str(cached).ok()?,
            output_per_million: Decimal::from_str(output).ok()?,
            multiplier: Decimal::from_str(multiplier).ok()?,
        })
    }

    pub fn estimate(&self, tokens: TokenTotals) -> Decimal {
        let million = Decimal::from(1_000_000_u64);
        let fresh = Decimal::from(tokens.fresh_input_tokens()) * self.input_per_million / million;
        let cached =
            Decimal::from(tokens.cached_input_tokens) * self.cached_input_per_million / million;
        let output = Decimal::from(tokens.output_tokens) * self.output_per_million / million;
        (fresh + cached + output) * self.multiplier
    }
}

pub const DEFAULT_PRICES: &[(&str, &str, &str, &str, &str)] = &[
    ("gpt-5", "GPT-5", "1.25", "0.125", "10"),
    ("gpt-5-codex", "GPT-5 Codex", "1.25", "0.125", "10"),
    ("gpt-5.1", "GPT-5.1", "1.25", "0.125", "10"),
    ("gpt-5.1-codex", "GPT-5.1 Codex", "1.25", "0.125", "10"),
    ("gpt-5.2", "GPT-5.2", "1.75", "0.175", "14"),
    ("gpt-5.2-codex", "GPT-5.2 Codex", "1.75", "0.175", "14"),
    ("gpt-5.3-codex", "GPT-5.3 Codex", "1.75", "0.175", "14"),
    ("gpt-5.4", "GPT-5.4", "2.50", "0.25", "15"),
    ("claude-opus-4-1", "Claude Opus 4.1", "15", "1.5", "75"),
    ("claude-opus-4", "Claude Opus 4", "15", "1.5", "75"),
    ("claude-sonnet-4", "Claude Sonnet 4", "3", "0.3", "15"),
    ("claude-3-7-sonnet", "Claude 3.7 Sonnet", "3", "0.3", "15"),
    ("claude-3-5-sonnet", "Claude 3.5 Sonnet", "3", "0.3", "15"),
    ("claude-3-5-haiku", "Claude 3.5 Haiku", "0.8", "0.08", "4"),
];
