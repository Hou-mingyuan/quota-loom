use crate::core::pricing::{CatalogModelPrice, ModelPrice, DEFAULT_PRICES};
use crate::core::types::{
    DailyCostPoint, DataSourceKind, ModelPriceEntry, ModelUsage, ParseOutcome, RecentUsageEvent,
    SessionCursor, TokenTotals, UsageRange, UsageSnapshot, UsageSummary, UsageTrendPoint,
};
use rusqlite::{params, Connection, OptionalExtension};
use rust_decimal::Decimal;
use std::path::Path;
use std::sync::Mutex;

pub struct UsageDatabase {
    connection: Mutex<Connection>,
}

impl UsageDatabase {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("无法创建数据目录 {}: {error}", parent.display()))?;
        }
        let connection = Connection::open(path)
            .map_err(|error| format!("无法打开统计数据库 {}: {error}", path.display()))?;
        connection
            .busy_timeout(std::time::Duration::from_secs(3))
            .map_err(|error| error.to_string())?;
        connection
            .execute_batch(
                "PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL; PRAGMA foreign_keys=ON;",
            )
            .map_err(|error| error.to_string())?;
        let database = Self {
            connection: Mutex::new(connection),
        };
        database.initialize()?;
        Ok(database)
    }

    pub fn open_in_memory() -> Result<Self, String> {
        let database = Self {
            connection: Mutex::new(
                Connection::open_in_memory().map_err(|error| error.to_string())?,
            ),
        };
        database.initialize()?;
        Ok(database)
    }

    fn initialize(&self) -> Result<(), String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        connection
            .execute_batch(
                "CREATE TABLE IF NOT EXISTS usage_events (
                    id TEXT PRIMARY KEY,
                    source_key TEXT NOT NULL,
                    thread_id TEXT NOT NULL,
                    event_index INTEGER NOT NULL,
                    occurred_at INTEGER NOT NULL,
                    model TEXT NOT NULL,
                    input_tokens INTEGER NOT NULL,
                    cached_input_tokens INTEGER NOT NULL,
                    output_tokens INTEGER NOT NULL,
                    source_file TEXT NOT NULL
                );
                CREATE INDEX IF NOT EXISTS idx_usage_events_time ON usage_events(occurred_at);
                CREATE INDEX IF NOT EXISTS idx_usage_events_model_time ON usage_events(model, occurred_at);
                CREATE INDEX IF NOT EXISTS idx_usage_events_thread_time ON usage_events(thread_id, occurred_at);
                CREATE INDEX IF NOT EXISTS idx_usage_events_source ON usage_events(source_key);

                CREATE TABLE IF NOT EXISTS session_cursors (
                    source_key TEXT PRIMARY KEY,
                    path TEXT NOT NULL,
                    file_size INTEGER NOT NULL,
                    modified_ns INTEGER NOT NULL,
                    byte_offset INTEGER NOT NULL,
                    event_index INTEGER NOT NULL,
                    thread_id TEXT,
                    current_model TEXT NOT NULL,
                    previous_input INTEGER,
                    previous_cached INTEGER,
                    previous_output INTEGER,
                    replay_active INTEGER NOT NULL DEFAULT 0,
                    updated_at INTEGER NOT NULL
                );

                CREATE TABLE IF NOT EXISTS model_prices (
                    model_id TEXT PRIMARY KEY,
                    display_name TEXT NOT NULL,
                    input_per_million TEXT NOT NULL,
                    cached_input_per_million TEXT NOT NULL,
                    output_per_million TEXT NOT NULL,
                    multiplier TEXT NOT NULL DEFAULT '1',
                    updated_at INTEGER NOT NULL
                );",
            )
            .map_err(|error| error.to_string())?;
        let has_multiplier = connection
            .query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM pragma_table_info('model_prices') WHERE name = 'multiplier'
                 )",
                [],
                |row| Ok(row.get::<_, i64>(0)? != 0),
            )
            .map_err(|error| error.to_string())?;
        if !has_multiplier {
            connection
                .execute(
                    "ALTER TABLE model_prices ADD COLUMN multiplier TEXT NOT NULL DEFAULT '1'",
                    [],
                )
                .map_err(|error| error.to_string())?;
        }
        for (model, display, input, cached, output) in DEFAULT_PRICES {
            connection
                .execute(
                    "INSERT OR IGNORE INTO model_prices
                     (model_id, display_name, input_per_million, cached_input_per_million,
                      output_per_million, multiplier, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, '1', strftime('%s','now'))",
                    params![model, display, input, cached, output],
                )
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    pub fn load_cursor(&self, source_key: &str) -> Result<Option<SessionCursor>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        connection
            .query_row(
                "SELECT source_key, path, file_size, modified_ns, byte_offset, event_index,
                        thread_id, current_model, previous_input, previous_cached, previous_output,
                        replay_active
                 FROM session_cursors WHERE source_key = ?1",
                [source_key],
                |row| {
                    let previous_input = row.get::<_, Option<i64>>(8)?;
                    let previous_cached = row.get::<_, Option<i64>>(9)?;
                    let previous_output = row.get::<_, Option<i64>>(10)?;
                    let previous_total = match (previous_input, previous_cached, previous_output) {
                        (Some(input), Some(cached), Some(output)) => Some(TokenTotals {
                            input_tokens: input.max(0) as u64,
                            cached_input_tokens: cached.max(0) as u64,
                            output_tokens: output.max(0) as u64,
                        }),
                        _ => None,
                    };
                    Ok(SessionCursor {
                        source_key: row.get(0)?,
                        path: row.get(1)?,
                        file_size: row.get::<_, i64>(2)?.max(0) as u64,
                        modified_ns: row.get::<_, i64>(3)?.max(0) as u64,
                        byte_offset: row.get::<_, i64>(4)?.max(0) as u64,
                        event_index: row.get::<_, i64>(5)?.max(0) as u64,
                        thread_id: row.get(6)?,
                        current_model: row.get(7)?,
                        previous_total,
                        replay_active: row.get::<_, i64>(11)? != 0,
                    })
                },
            )
            .optional()
            .map_err(|error| error.to_string())
    }

    pub fn apply_parse_outcome(&self, outcome: &ParseOutcome) -> Result<u64, String> {
        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        if outcome.reset_required {
            transaction
                .execute(
                    "DELETE FROM usage_events WHERE source_key = ?1",
                    [&outcome.cursor.source_key],
                )
                .map_err(|error| error.to_string())?;
        }
        let mut inserted = 0_u64;
        for event in &outcome.events {
            inserted += transaction
                .execute(
                    "INSERT OR IGNORE INTO usage_events
                     (id, source_key, thread_id, event_index, occurred_at, model,
                      input_tokens, cached_input_tokens, output_tokens, source_file)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    params![
                        event.id,
                        event.source_key,
                        event.thread_id,
                        event.event_index as i64,
                        event.occurred_at,
                        event.model,
                        event.tokens.input_tokens as i64,
                        event.tokens.cached_input_tokens as i64,
                        event.tokens.output_tokens as i64,
                        event.source_file,
                    ],
                )
                .map_err(|error| error.to_string())? as u64;
        }
        let previous = outcome.cursor.previous_total;
        transaction
            .execute(
                "INSERT INTO session_cursors
                 (source_key, path, file_size, modified_ns, byte_offset, event_index, thread_id,
                  current_model, previous_input, previous_cached, previous_output, replay_active, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, strftime('%s','now'))
                 ON CONFLICT(source_key) DO UPDATE SET
                    path=excluded.path, file_size=excluded.file_size, modified_ns=excluded.modified_ns,
                    byte_offset=excluded.byte_offset, event_index=excluded.event_index,
                    thread_id=excluded.thread_id, current_model=excluded.current_model,
                    previous_input=excluded.previous_input, previous_cached=excluded.previous_cached,
                    previous_output=excluded.previous_output, replay_active=excluded.replay_active,
                    updated_at=excluded.updated_at",
                params![
                    outcome.cursor.source_key,
                    outcome.cursor.path,
                    outcome.cursor.file_size as i64,
                    outcome.cursor.modified_ns.min(i64::MAX as u64) as i64,
                    outcome.cursor.byte_offset as i64,
                    outcome.cursor.event_index as i64,
                    outcome.cursor.thread_id,
                    outcome.cursor.current_model,
                    previous.map(|tokens| tokens.input_tokens as i64),
                    previous.map(|tokens| tokens.cached_input_tokens as i64),
                    previous.map(|tokens| tokens.output_tokens as i64),
                    i64::from(outcome.cursor.replay_active),
                ],
            )
            .map_err(|error| error.to_string())?;
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(inserted)
    }

    pub fn clear_usage(&self) -> Result<(), String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        connection
            .execute_batch("DELETE FROM usage_events; DELETE FROM session_cursors;")
            .map_err(|error| error.to_string())
    }

    pub fn used_model_prices(&self) -> Result<Vec<ModelPriceEntry>, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut statement = connection
            .prepare(
                "SELECT used.model,
                        COALESCE(prices.input_per_million, ''),
                        COALESCE(prices.cached_input_per_million, ''),
                        COALESCE(prices.output_per_million, ''),
                        COALESCE(prices.multiplier, '1'),
                        prices.model_id IS NOT NULL
                 FROM (SELECT DISTINCT model FROM usage_events) AS used
                 LEFT JOIN model_prices AS prices ON prices.model_id = used.model
                 ORDER BY used.model COLLATE NOCASE",
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map([], |row| {
                Ok(ModelPriceEntry {
                    model: row.get(0)?,
                    input_per_million: row.get(1)?,
                    cached_input_per_million: row.get(2)?,
                    output_per_million: row.get(3)?,
                    multiplier: row.get(4)?,
                    configured: row.get(5)?,
                })
            })
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub fn import_catalog_prices(&self, prices: &[CatalogModelPrice]) -> Result<u64, String> {
        let mut connection = self.connection.lock().map_err(|error| error.to_string())?;
        let transaction = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let mut imported = 0_u64;
        for price in prices {
            imported += transaction
                .execute(
                    "INSERT INTO model_prices
                     (model_id, display_name, input_per_million, cached_input_per_million,
                      output_per_million, multiplier, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, '1', strftime('%s','now'))
                     ON CONFLICT(model_id) DO UPDATE SET
                        display_name=excluded.display_name,
                        input_per_million=excluded.input_per_million,
                        cached_input_per_million=excluded.cached_input_per_million,
                        output_per_million=excluded.output_per_million,
                        updated_at=excluded.updated_at",
                    params![
                        price.model,
                        price.display_name,
                        price.input_per_million,
                        price.cached_input_per_million,
                        price.output_per_million,
                    ],
                )
                .map_err(|error| error.to_string())? as u64;
        }
        transaction.commit().map_err(|error| error.to_string())?;
        Ok(imported)
    }

    pub fn update_model_multiplier(&self, model: &str, multiplier: &str) -> Result<(), String> {
        let multiplier = multiplier
            .parse::<Decimal>()
            .map_err(|_| "倍率必须是有效数字".to_string())?;
        if multiplier.is_sign_negative() {
            return Err("倍率不能为负数".to_string());
        }
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let updated = connection
            .execute(
                "UPDATE model_prices SET multiplier = ?1, updated_at = strftime('%s','now')
                 WHERE model_id = ?2",
                params![multiplier.normalize().to_string(), model],
            )
            .map_err(|error| error.to_string())?;
        if updated == 0 {
            return Err(format!("models.dev 中没有模型 {model} 的价格"));
        }
        Ok(())
    }

    pub fn snapshot(
        &self,
        codex_home: &Path,
        source_kind: DataSourceKind,
        range: UsageRange,
    ) -> Result<UsageSnapshot, String> {
        let connection = self.connection.lock().map_err(|error| error.to_string())?;
        let mut models = query_models(&connection, range)?;
        let prices = load_prices(&connection)?;
        let threads = connection
            .query_row(
                "SELECT COUNT(DISTINCT thread_id) FROM usage_events
                 WHERE occurred_at >= ?1 AND occurred_at <= ?2",
                params![range.start_at, range.end_at],
                |row| row.get::<_, i64>(0),
            )
            .map_err(|error| error.to_string())?
            .max(0) as u64;

        let mut summary = UsageSummary {
            threads,
            ..UsageSummary::default()
        };
        let mut total_cost = Decimal::ZERO;
        let mut all_priced = true;
        for model in &mut models {
            let totals = TokenTotals {
                input_tokens: model
                    .fresh_input_tokens
                    .saturating_add(model.cached_input_tokens),
                cached_input_tokens: model.cached_input_tokens,
                output_tokens: model.output_tokens,
            };
            let cost = prices.get(&model.model).map(|price| price.estimate(totals));
            model.estimated_cost_usd = cost.map(format_cost);
            if let Some(cost) = cost {
                total_cost += cost;
            } else {
                all_priced = false;
            }
            summary.total_tokens = summary.total_tokens.saturating_add(model.total_tokens);
            summary.fresh_input_tokens = summary
                .fresh_input_tokens
                .saturating_add(model.fresh_input_tokens);
            summary.cached_input_tokens = summary
                .cached_input_tokens
                .saturating_add(model.cached_input_tokens);
            summary.output_tokens = summary.output_tokens.saturating_add(model.output_tokens);
            summary.calls = summary.calls.saturating_add(model.calls);
        }
        let input_total = summary
            .fresh_input_tokens
            .saturating_add(summary.cached_input_tokens);
        summary.cache_hit_rate = if input_total > 0 {
            summary.cached_input_tokens as f64 / input_total as f64
        } else {
            0.0
        };
        summary.estimated_cost_usd = all_priced.then(|| format_cost(total_cost));

        let mut recent = query_recent(&connection, range, 30)?;
        for event in &mut recent {
            let totals = TokenTotals {
                input_tokens: event
                    .fresh_input_tokens
                    .saturating_add(event.cached_input_tokens),
                cached_input_tokens: event.cached_input_tokens,
                output_tokens: event.output_tokens,
            };
            event.estimated_cost_usd = prices
                .get(&event.model)
                .map(|price| format_cost(price.estimate(totals)));
        }

        Ok(UsageSnapshot {
            generated_at: chrono::Utc::now().timestamp(),
            codex_home: codex_home.to_string_lossy().to_string(),
            source_kind,
            source_label: source_kind.label().to_string(),
            source_brand: source_kind.brand().to_string(),
            summary,
            trends: query_trends(&connection, range)?,
            daily_costs: query_daily_costs(&connection, range, &prices)?,
            models,
            recent,
        })
    }
}

fn query_models(connection: &Connection, range: UsageRange) -> Result<Vec<ModelUsage>, String> {
    let mut statement = connection
        .prepare(
            "SELECT model, COALESCE(SUM(input_tokens),0), COALESCE(SUM(cached_input_tokens),0),
                    COALESCE(SUM(output_tokens),0), COUNT(*)
             FROM usage_events WHERE occurred_at >= ?1 AND occurred_at <= ?2
             GROUP BY model ORDER BY SUM(input_tokens + output_tokens) DESC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![range.start_at, range.end_at], |row| {
            let input = row.get::<_, i64>(1)?.max(0) as u64;
            let cached = (row.get::<_, i64>(2)?.max(0) as u64).min(input);
            let output = row.get::<_, i64>(3)?.max(0) as u64;
            Ok(ModelUsage {
                model: row.get(0)?,
                total_tokens: input.saturating_add(output),
                fresh_input_tokens: input.saturating_sub(cached),
                cached_input_tokens: cached,
                output_tokens: output,
                calls: row.get::<_, i64>(4)?.max(0) as u64,
                cache_hit_rate: if input > 0 {
                    cached as f64 / input as f64
                } else {
                    0.0
                },
                estimated_cost_usd: None,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn query_trends(
    connection: &Connection,
    range: UsageRange,
) -> Result<Vec<UsageTrendPoint>, String> {
    let bucket = range.bucket_seconds.max(60);
    let mut statement = connection
        .prepare(
            "SELECT (occurred_at / ?1) * ?1 AS bucket_start,
                    COALESCE(SUM(input_tokens),0), COALESCE(SUM(cached_input_tokens),0),
                    COALESCE(SUM(output_tokens),0), COUNT(*)
             FROM usage_events WHERE occurred_at >= ?2 AND occurred_at <= ?3
             GROUP BY bucket_start ORDER BY bucket_start ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![bucket, range.start_at, range.end_at], |row| {
            let input = row.get::<_, i64>(1)?.max(0) as u64;
            let cached = (row.get::<_, i64>(2)?.max(0) as u64).min(input);
            let output = row.get::<_, i64>(3)?.max(0) as u64;
            Ok(UsageTrendPoint {
                bucket_start: row.get(0)?,
                total_tokens: input.saturating_add(output),
                fresh_input_tokens: input.saturating_sub(cached),
                cached_input_tokens: cached,
                output_tokens: output,
                calls: row.get::<_, i64>(4)?.max(0) as u64,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn query_recent(
    connection: &Connection,
    range: UsageRange,
    limit: i64,
) -> Result<Vec<RecentUsageEvent>, String> {
    let mut statement = connection
        .prepare(
            "SELECT id, thread_id, occurred_at, model, input_tokens, cached_input_tokens, output_tokens
             FROM usage_events WHERE occurred_at >= ?1 AND occurred_at <= ?2
             ORDER BY occurred_at DESC, event_index DESC LIMIT ?3",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![range.start_at, range.end_at, limit], |row| {
            let input = row.get::<_, i64>(4)?.max(0) as u64;
            let cached = (row.get::<_, i64>(5)?.max(0) as u64).min(input);
            let output = row.get::<_, i64>(6)?.max(0) as u64;
            Ok(RecentUsageEvent {
                id: row.get(0)?,
                thread_id: row.get(1)?,
                occurred_at: row.get(2)?,
                model: row.get(3)?,
                total_tokens: input.saturating_add(output),
                fresh_input_tokens: input.saturating_sub(cached),
                cached_input_tokens: cached,
                output_tokens: output,
                estimated_cost_usd: None,
            })
        })
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn query_daily_costs(
    connection: &Connection,
    range: UsageRange,
    prices: &std::collections::HashMap<String, ModelPrice>,
) -> Result<Vec<DailyCostPoint>, String> {
    let offset = range.timezone_offset_seconds.clamp(-86_400, 86_400);
    let first_day_start = ((range.start_at.saturating_add(offset)) / 86_400) * 86_400 - offset;
    let mut statement = connection
        .prepare(
            "SELECT ((occurred_at + ?1) / 86400) * 86400 - ?1 AS day_start, model,
                    COALESCE(SUM(input_tokens),0), COALESCE(SUM(cached_input_tokens),0),
                    COALESCE(SUM(output_tokens),0)
             FROM usage_events WHERE occurred_at >= ?2 AND occurred_at <= ?3
             GROUP BY day_start, model ORDER BY day_start ASC",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map(params![offset, first_day_start, range.end_at], |row| {
            let input = row.get::<_, i64>(2)?.max(0) as u64;
            let cached = (row.get::<_, i64>(3)?.max(0) as u64).min(input);
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                TokenTotals {
                    input_tokens: input,
                    cached_input_tokens: cached,
                    output_tokens: row.get::<_, i64>(4)?.max(0) as u64,
                },
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut days = std::collections::BTreeMap::<i64, (Decimal, bool)>::new();
    for row in rows {
        let (day_start, model, tokens) = row.map_err(|error| error.to_string())?;
        let entry = days.entry(day_start).or_insert((Decimal::ZERO, true));
        if let Some(price) = prices.get(&model) {
            entry.0 += price.estimate(tokens);
        } else {
            entry.1 = false;
        }
    }

    Ok(days
        .into_iter()
        .map(|(day_start, (cost, all_priced))| DailyCostPoint {
            day_start,
            estimated_cost_usd: all_priced.then(|| format_cost(cost)),
        })
        .collect())
}

fn load_prices(
    connection: &Connection,
) -> Result<std::collections::HashMap<String, ModelPrice>, String> {
    let mut statement = connection
        .prepare(
            "SELECT model_id, input_per_million, cached_input_per_million, output_per_million,
                    multiplier
             FROM model_prices",
        )
        .map_err(|error| error.to_string())?;
    let rows = statement
        .query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
                row.get::<_, String>(4)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut prices = std::collections::HashMap::new();
    for row in rows {
        let (model, input, cached, output, multiplier) = row.map_err(|error| error.to_string())?;
        if let Some(price) = ModelPrice::from_strings(&input, &cached, &output, &multiplier) {
            prices.insert(model, price);
        }
    }
    Ok(prices)
}

fn format_cost(cost: Decimal) -> String {
    cost.round_dp(6).normalize().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::types::{ParseOutcome, SessionCursor, UsageEvent};

    #[test]
    fn builds_cache_normalized_summary() {
        let database = UsageDatabase::open_in_memory().unwrap();
        let event = UsageEvent {
            id: "codex:t:1".into(),
            source_key: "s".into(),
            thread_id: "t".into(),
            event_index: 1,
            occurred_at: 100,
            model: "gpt-5.3-codex".into(),
            tokens: TokenTotals {
                input_tokens: 1_000,
                cached_input_tokens: 750,
                output_tokens: 200,
            },
            source_file: "fixture.jsonl".into(),
        };
        database
            .apply_parse_outcome(&ParseOutcome {
                cursor: SessionCursor {
                    source_key: "s".into(),
                    path: "fixture.jsonl".into(),
                    current_model: "gpt-5.3-codex".into(),
                    ..SessionCursor::default()
                },
                events: vec![event],
                reset_required: false,
            })
            .unwrap();

        let snapshot = database
            .snapshot(
                Path::new("/tmp/.codex"),
                DataSourceKind::CodexCli,
                UsageRange {
                    start_at: 0,
                    end_at: 200,
                    bucket_seconds: 60,
                    timezone_offset_seconds: 0,
                },
            )
            .unwrap();
        assert_eq!(snapshot.summary.total_tokens, 1_200);
        assert_eq!(snapshot.summary.fresh_input_tokens, 250);
        assert_eq!(snapshot.summary.cached_input_tokens, 750);
        assert_eq!(snapshot.summary.output_tokens, 200);
        assert_eq!(snapshot.summary.cache_hit_rate, 0.75);
        assert!(snapshot.summary.estimated_cost_usd.is_some());
        assert_eq!(snapshot.daily_costs.len(), 1);
        assert!(snapshot.daily_costs[0].estimated_cost_usd.is_some());
        assert!(snapshot.recent[0].estimated_cost_usd.is_some());
    }

    #[test]
    fn lists_used_models_and_applies_custom_prices() {
        let database = UsageDatabase::open_in_memory().unwrap();
        database
            .apply_parse_outcome(&ParseOutcome {
                cursor: SessionCursor {
                    source_key: "custom-source".into(),
                    path: "custom.jsonl".into(),
                    current_model: "custom-codex".into(),
                    ..SessionCursor::default()
                },
                events: vec![UsageEvent {
                    id: "codex:custom:1".into(),
                    source_key: "custom-source".into(),
                    thread_id: "custom".into(),
                    event_index: 1,
                    occurred_at: 100,
                    model: "custom-codex".into(),
                    tokens: TokenTotals {
                        input_tokens: 1_000_000,
                        cached_input_tokens: 0,
                        output_tokens: 1_000_000,
                    },
                    source_file: "custom.jsonl".into(),
                }],
                reset_required: false,
            })
            .unwrap();

        let prices = database.used_model_prices().unwrap();
        assert_eq!(prices.len(), 1);
        assert_eq!(prices[0].model, "custom-codex");
        assert!(!prices[0].configured);

        database
            .import_catalog_prices(&[CatalogModelPrice {
                model: "custom-codex".into(),
                display_name: "Custom Codex".into(),
                input_per_million: "2".into(),
                cached_input_per_million: "0.2".into(),
                output_per_million: "10".into(),
            }])
            .unwrap();
        database
            .update_model_multiplier("custom-codex", "0.5")
            .unwrap();
        let snapshot = database
            .snapshot(
                Path::new("/tmp/.codex"),
                DataSourceKind::CodexCli,
                UsageRange {
                    start_at: 0,
                    end_at: 200,
                    bucket_seconds: 60,
                    timezone_offset_seconds: 0,
                },
            )
            .unwrap();
        assert_eq!(snapshot.summary.estimated_cost_usd.as_deref(), Some("6"));
        assert_eq!(
            snapshot.daily_costs[0].estimated_cost_usd.as_deref(),
            Some("6")
        );
    }
}
