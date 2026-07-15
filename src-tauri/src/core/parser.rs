use crate::core::types::{DataSourceKind, ParseOutcome, SessionCursor, TokenTotals, UsageEvent};
use chrono::DateTime;
use serde_json::Value;
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::Path;
use std::time::UNIX_EPOCH;

pub fn source_key(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| path.to_string_lossy().into_owned())
}

pub fn normalize_model(raw: &str) -> String {
    let mut model = raw.trim().to_ascii_lowercase();
    if let Some((_, suffix)) = model.rsplit_once('/') {
        model = suffix.to_string();
    }
    for suffix_len in [11, 9] {
        if model.len() <= suffix_len || !model.is_char_boundary(model.len() - suffix_len) {
            continue;
        }
        let suffix = &model[model.len() - suffix_len..];
        let is_date = if suffix_len == 11 {
            let bytes = suffix.as_bytes();
            bytes[0] == b'-'
                && bytes[1..5].iter().all(u8::is_ascii_digit)
                && bytes[5] == b'-'
                && bytes[6..8].iter().all(u8::is_ascii_digit)
                && bytes[8] == b'-'
                && bytes[9..11].iter().all(u8::is_ascii_digit)
        } else {
            suffix.as_bytes()[0] == b'-' && suffix.as_bytes()[1..].iter().all(u8::is_ascii_digit)
        };
        if is_date {
            model.truncate(model.len() - suffix_len);
            break;
        }
    }
    if model.is_empty() {
        "unknown".to_string()
    } else {
        model
    }
}

pub fn parse_session_file(
    path: &Path,
    previous_cursor: Option<SessionCursor>,
) -> Result<ParseOutcome, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("无法读取会话文件元数据 {}: {error}", path.display()))?;
    let file_size = metadata.len();
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos().min(u64::MAX as u128) as u64)
        .unwrap_or(0);
    let key = source_key(path);
    let reset_required = previous_cursor
        .as_ref()
        .is_some_and(|cursor| file_size < cursor.byte_offset);
    let mut cursor = if reset_required {
        SessionCursor::default()
    } else {
        previous_cursor.unwrap_or_default()
    };
    cursor.source_key = key.clone();
    cursor.path = path.to_string_lossy().to_string();
    if cursor.current_model.is_empty() {
        cursor.current_model = "unknown".to_string();
    }

    if !reset_required && cursor.file_size == file_size && cursor.modified_ns == modified_ns {
        return Ok(ParseOutcome {
            cursor,
            events: Vec::new(),
            reset_required: false,
        });
    }

    let mut file = fs::File::open(path)
        .map_err(|error| format!("无法打开会话文件 {}: {error}", path.display()))?;
    file.seek(SeekFrom::Start(cursor.byte_offset))
        .map_err(|error| format!("无法定位会话文件 {}: {error}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut events = Vec::new();
    let mut consumed_offset = cursor.byte_offset;

    loop {
        let mut bytes = Vec::new();
        let read = reader
            .read_until(b'\n', &mut bytes)
            .map_err(|error| format!("读取会话文件失败 {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        if !bytes.ends_with(b"\n") {
            break;
        }
        consumed_offset = consumed_offset.saturating_add(read as u64);
        let Ok(line) = std::str::from_utf8(&bytes) else {
            continue;
        };
        process_line(line.trim_end(), &key, path, &mut cursor, &mut events);
    }

    cursor.byte_offset = consumed_offset;
    cursor.file_size = file_size;
    cursor.modified_ns = modified_ns;

    Ok(ParseOutcome {
        cursor,
        events,
        reset_required,
    })
}

pub fn parse_session_file_for_source(
    path: &Path,
    previous_cursor: Option<SessionCursor>,
    source_kind: DataSourceKind,
) -> Result<ParseOutcome, String> {
    match source_kind {
        DataSourceKind::ClaudeCode => parse_claude_session_file(path, previous_cursor),
        DataSourceKind::CodexCli | DataSourceKind::ChatGptCodex => {
            parse_session_file(path, previous_cursor)
        }
    }
}

fn parse_claude_session_file(
    path: &Path,
    previous_cursor: Option<SessionCursor>,
) -> Result<ParseOutcome, String> {
    let metadata = fs::metadata(path)
        .map_err(|error| format!("无法读取会话文件元数据 {}: {error}", path.display()))?;
    let file_size = metadata.len();
    let modified_ns = metadata
        .modified()
        .ok()
        .and_then(|value| value.duration_since(UNIX_EPOCH).ok())
        .map(|value| value.as_nanos().min(u64::MAX as u128) as u64)
        .unwrap_or(0);
    let key = source_key(path);
    let reset_required = previous_cursor
        .as_ref()
        .is_some_and(|cursor| file_size < cursor.byte_offset);
    let mut cursor = if reset_required {
        SessionCursor::default()
    } else {
        previous_cursor.unwrap_or_default()
    };
    cursor.source_key = key.clone();
    cursor.path = path.to_string_lossy().to_string();
    if cursor.current_model.is_empty() {
        cursor.current_model = "unknown".to_string();
    }

    if !reset_required && cursor.file_size == file_size && cursor.modified_ns == modified_ns {
        return Ok(ParseOutcome {
            cursor,
            events: Vec::new(),
            reset_required: false,
        });
    }

    let mut file = fs::File::open(path)
        .map_err(|error| format!("无法打开会话文件 {}: {error}", path.display()))?;
    file.seek(SeekFrom::Start(cursor.byte_offset))
        .map_err(|error| format!("无法定位会话文件 {}: {error}", path.display()))?;
    let mut reader = BufReader::new(file);
    let mut events = Vec::new();
    let mut consumed_offset = cursor.byte_offset;

    loop {
        let mut bytes = Vec::new();
        let read = reader
            .read_until(b'\n', &mut bytes)
            .map_err(|error| format!("读取会话文件失败 {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        if !bytes.ends_with(b"\n") {
            break;
        }
        consumed_offset = consumed_offset.saturating_add(read as u64);
        let Ok(line) = std::str::from_utf8(&bytes) else {
            continue;
        };
        process_claude_line(line.trim_end(), &key, path, &mut cursor, &mut events);
    }

    cursor.byte_offset = consumed_offset;
    cursor.file_size = file_size;
    cursor.modified_ns = modified_ns;

    Ok(ParseOutcome {
        cursor,
        events,
        reset_required,
    })
}

fn process_claude_line(
    line: &str,
    source_key: &str,
    path: &Path,
    cursor: &mut SessionCursor,
    events: &mut Vec<UsageEvent>,
) {
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return;
    };
    if value.get("type").and_then(Value::as_str) != Some("assistant") {
        return;
    }
    let Some(message) = value.get("message") else {
        return;
    };
    let Some(usage) = message.get("usage") else {
        return;
    };
    let input_tokens = number(usage.get("input_tokens")).unwrap_or(0);
    let cached_input_tokens = number(usage.get("cache_read_input_tokens")).unwrap_or(0);
    let cache_creation_tokens = number(usage.get("cache_creation_input_tokens")).unwrap_or(0);
    let output_tokens = number(usage.get("output_tokens")).unwrap_or(0);
    let delta = TokenTotals {
        input_tokens: input_tokens
            .saturating_add(cached_input_tokens)
            .saturating_add(cache_creation_tokens),
        cached_input_tokens,
        output_tokens,
    };
    if delta.is_zero() {
        return;
    }
    if let Some(model) = message.get("model").and_then(Value::as_str) {
        cursor.current_model = normalize_model(model);
    }
    let thread_id = value
        .get("sessionId")
        .or_else(|| value.get("session_id"))
        .or_else(|| value.get("sessionID"))
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .or_else(|| cursor.thread_id.clone())
        .unwrap_or_else(|| source_key.to_string());
    cursor.thread_id = Some(thread_id.clone());
    cursor.event_index = cursor.event_index.saturating_add(1);
    let occurred_at = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|timestamp| DateTime::parse_from_rfc3339(timestamp).ok())
        .map(|timestamp| timestamp.timestamp())
        .unwrap_or_else(|| chrono::Utc::now().timestamp());
    let message_id = message.get("id").and_then(Value::as_str);
    let event_id = message_id
        .map(|id| format!("claude:{id}"))
        .unwrap_or_else(|| format!("claude:{thread_id}:{}", cursor.event_index));
    events.push(UsageEvent {
        id: event_id,
        source_key: source_key.to_string(),
        thread_id,
        event_index: cursor.event_index,
        occurred_at,
        model: cursor.current_model.clone(),
        tokens: delta,
        source_file: path.to_string_lossy().to_string(),
    });
}

fn process_line(
    line: &str,
    source_key: &str,
    path: &Path,
    cursor: &mut SessionCursor,
    events: &mut Vec<UsageEvent>,
) {
    if line.is_empty() {
        return;
    }
    if !line.contains("session_meta")
        && !line.contains("turn_context")
        && !line.contains("token_count")
        && !line.contains("thread_settings_applied")
        && !line.contains("inter_agent_communication")
    {
        return;
    }
    let Ok(value) = serde_json::from_str::<Value>(line) else {
        return;
    };
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();

    if event_type.starts_with("inter_agent_communication")
        || (event_type == "event_msg"
            && value.pointer("/payload/type").and_then(Value::as_str)
                == Some("thread_settings_applied"))
    {
        cursor.replay_active = false;
        return;
    }

    match event_type {
        "session_meta" => {
            if let Some(payload) = value.get("payload") {
                if let Some(identity) = session_identity(payload) {
                    if cursor.thread_id.is_none() {
                        cursor.thread_id = Some(identity.thread_id);
                    }
                    if cursor.event_index == 0 && identity.carries_history_snapshot {
                        cursor.replay_active = true;
                    }
                }
            }
        }
        "turn_context" => {
            if let Some(model) = value
                .pointer("/payload/model")
                .or_else(|| value.pointer("/payload/info/model"))
                .and_then(Value::as_str)
            {
                cursor.current_model = normalize_model(model);
            }
        }
        "event_msg" => process_token_event(value, source_key, path, cursor, events),
        _ => {}
    }
}

fn process_token_event(
    value: Value,
    source_key: &str,
    path: &Path,
    cursor: &mut SessionCursor,
    events: &mut Vec<UsageEvent>,
) {
    if value.pointer("/payload/type").and_then(Value::as_str) != Some("token_count") {
        return;
    }
    let Some(info) = value
        .pointer("/payload/info")
        .filter(|value| !value.is_null())
    else {
        return;
    };
    if let Some(model) = info
        .get("model")
        .or_else(|| info.get("model_name"))
        .or_else(|| value.pointer("/payload/model"))
        .and_then(Value::as_str)
    {
        cursor.current_model = normalize_model(model);
    }

    let total = parse_totals(info.get("total_token_usage"));
    let last = parse_totals(info.get("last_token_usage"));
    let delta = match total {
        Some(total) => {
            let delta = cursor
                .previous_total
                .and_then(|previous| subtract_totals(total, previous))
                .or(last)
                .unwrap_or(total);
            cursor.previous_total = Some(total);
            delta
        }
        None => match last {
            Some(last) => last,
            None => return,
        },
    };
    let delta = TokenTotals {
        cached_input_tokens: delta.cached_input_tokens.min(delta.input_tokens),
        ..delta
    };
    if delta.is_zero() {
        return;
    }

    cursor.event_index = cursor.event_index.saturating_add(1);
    if cursor.replay_active {
        return;
    }
    let thread_id = cursor
        .thread_id
        .clone()
        .unwrap_or_else(|| source_key.to_string());
    let occurred_at = value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(|timestamp| DateTime::parse_from_rfc3339(timestamp).ok())
        .map(|timestamp| timestamp.timestamp())
        .unwrap_or_else(|| chrono::Utc::now().timestamp());
    let event_index = cursor.event_index;
    events.push(UsageEvent {
        id: format!("codex:{thread_id}:{event_index}"),
        source_key: source_key.to_string(),
        thread_id,
        event_index,
        occurred_at,
        model: cursor.current_model.clone(),
        tokens: delta,
        source_file: path.to_string_lossy().to_string(),
    });
}

fn subtract_totals(current: TokenTotals, previous: TokenTotals) -> Option<TokenTotals> {
    if current.input_tokens < previous.input_tokens
        || current.cached_input_tokens < previous.cached_input_tokens
        || current.output_tokens < previous.output_tokens
    {
        return None;
    }
    Some(TokenTotals {
        input_tokens: current.input_tokens - previous.input_tokens,
        cached_input_tokens: current.cached_input_tokens - previous.cached_input_tokens,
        output_tokens: current.output_tokens - previous.output_tokens,
    })
}

fn parse_totals(value: Option<&Value>) -> Option<TokenTotals> {
    let value = value?;
    Some(TokenTotals {
        input_tokens: number(value.get("input_tokens")).unwrap_or(0),
        cached_input_tokens: number(value.get("cached_input_tokens")).unwrap_or(0),
        output_tokens: number(value.get("output_tokens")).unwrap_or(0),
    })
}

fn number(value: Option<&Value>) -> Option<u64> {
    let value = value?;
    value
        .as_u64()
        .or_else(|| value.as_i64().and_then(|number| u64::try_from(number).ok()))
        .or_else(|| value.as_str().and_then(|number| number.parse().ok()))
}

struct SessionIdentity {
    thread_id: String,
    carries_history_snapshot: bool,
}

fn session_identity(payload: &Value) -> Option<SessionIdentity> {
    let thread_id = payload
        .get("id")
        .or_else(|| payload.get("thread_id"))
        .or_else(|| payload.get("threadId"))
        .or_else(|| payload.get("session_id"))
        .or_else(|| payload.get("sessionId"))
        .and_then(Value::as_str)?
        .to_string();
    let parent = payload
        .get("session_id")
        .or_else(|| payload.get("sessionId"))
        .and_then(Value::as_str);
    let carries_history_snapshot = payload
        .get("forked_from_id")
        .and_then(Value::as_str)
        .is_some_and(|value| !value.is_empty())
        || payload.pointer("/source/subagent").is_some()
        || parent.is_some_and(|parent| parent != thread_id);
    Some(SessionIdentity {
        thread_id,
        carries_history_snapshot,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn line(value: Value) -> String {
        format!("{}\n", serde_json::to_string(&value).unwrap())
    }

    #[test]
    fn parses_cumulative_deltas_and_model_changes() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            "{}{}{}{}",
            line(serde_json::json!({"type":"session_meta","payload":{"id":"thread-a"}})),
            line(serde_json::json!({"type":"turn_context","payload":{"model":"openai/gpt-5.3-codex-20260305"}})),
            line(serde_json::json!({"type":"event_msg","timestamp":"2026-07-14T08:00:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":20},"last_token_usage":{"input_tokens":100,"cached_input_tokens":40,"output_tokens":20}}}})),
            line(serde_json::json!({"type":"event_msg","timestamp":"2026-07-14T08:01:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":250,"cached_input_tokens":90,"output_tokens":55},"last_token_usage":{"input_tokens":150,"cached_input_tokens":50,"output_tokens":35}}}})),
        )
        .unwrap();

        let outcome = parse_session_file(file.path(), None).unwrap();
        assert_eq!(outcome.events.len(), 2);
        assert_eq!(outcome.events[0].model, "gpt-5.3-codex");
        assert_eq!(outcome.events[1].tokens.input_tokens, 150);
        assert_eq!(outcome.events[1].tokens.cached_input_tokens, 50);
        assert_eq!(outcome.events[1].tokens.output_tokens, 35);
    }

    #[test]
    fn parses_claude_usage_records() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            "{}{}",
            line(serde_json::json!({
                "type":"user",
                "sessionId":"claude-session"
            })),
            line(serde_json::json!({
                "type":"assistant",
                "sessionId":"claude-session",
                "timestamp":"2026-07-15T08:00:00Z",
                "message":{
                    "id":"msg-1",
                    "model":"claude-sonnet-4-20250514",
                    "usage":{
                        "input_tokens":100,
                        "cache_read_input_tokens":40,
                        "cache_creation_input_tokens":10,
                        "output_tokens":20
                    }
                }
            }))
        )
        .unwrap();

        let outcome =
            parse_session_file_for_source(file.path(), None, DataSourceKind::ClaudeCode).unwrap();
        assert_eq!(outcome.events.len(), 1);
        assert_eq!(outcome.events[0].id, "claude:msg-1");
        assert_eq!(outcome.events[0].thread_id, "claude-session");
        assert_eq!(outcome.events[0].model, "claude-sonnet-4");
        assert_eq!(outcome.events[0].tokens.input_tokens, 150);
        assert_eq!(outcome.events[0].tokens.cached_input_tokens, 40);
        assert_eq!(outcome.events[0].tokens.output_tokens, 20);
    }

    #[test]
    fn ignores_incomplete_tail_until_newline_arrives() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            "{}",
            line(serde_json::json!({"type":"session_meta","payload":{"id":"thread-b"}}))
        )
        .unwrap();
        write!(file, "{{\"type\":\"event_msg\"").unwrap();

        let outcome = parse_session_file(file.path(), None).unwrap();
        assert!(outcome.events.is_empty());
        assert!(outcome.cursor.byte_offset < outcome.cursor.file_size);
    }

    #[test]
    fn skips_fork_history_before_takeover_boundary() {
        let mut file = NamedTempFile::new().unwrap();
        write!(
            file,
            "{}{}{}{}",
            line(serde_json::json!({"type":"session_meta","payload":{"id":"child","session_id":"parent","source":{"subagent":{}}}})),
            line(serde_json::json!({"type":"event_msg","timestamp":"2026-07-14T08:00:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":100,"cached_input_tokens":20,"output_tokens":10}}}})),
            line(serde_json::json!({"type":"event_msg","payload":{"type":"thread_settings_applied"}})),
            line(serde_json::json!({"type":"event_msg","timestamp":"2026-07-14T08:02:00Z","payload":{"type":"token_count","info":{"total_token_usage":{"input_tokens":180,"cached_input_tokens":30,"output_tokens":25},"last_token_usage":{"input_tokens":80,"cached_input_tokens":10,"output_tokens":15}}}})),
        )
        .unwrap();

        let outcome = parse_session_file(file.path(), None).unwrap();
        assert_eq!(outcome.events.len(), 1);
        assert_eq!(outcome.events[0].thread_id, "child");
        assert_eq!(outcome.events[0].tokens.input_tokens, 80);
    }
}
