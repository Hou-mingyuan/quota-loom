use crate::core::database::UsageDatabase;
use crate::core::parser::{parse_session_file_for_source, source_key};
use crate::core::pricing::CatalogModelPrice;
use crate::core::types::{
    DataSourceKind, ModelPriceEntry, SyncResult, UsageRange, UsageSnapshot, WeeklyUsage,
};
use directories::{ProjectDirs, UserDirs};
use notify::{PollWatcher, RecommendedWatcher, RecursiveMode, Watcher};
use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};
use walkdir::WalkDir;

const WEEKLY_WINDOW_MINUTES: i64 = 7 * 24 * 60;
const WEEKLY_USAGE_CACHE_TTL: Duration = Duration::from_secs(60);
const APP_SERVER_TIMEOUT: Duration = Duration::from_secs(8);

const MODELS_DEV_PROVIDERS: &[&str] = &[
    "openai",
    "anthropic",
    "deepseek",
    "google",
    "google-vertex",
    "google-vertex-anthropic",
    "mistral",
    "cohere",
    "xai",
    "groq",
    "meta",
    "llama",
    "alibaba",
    "alibaba-cn",
    "kimi-for-coding",
    "moonshotai",
    "moonshotai-cn",
    "minimax",
    "minimax-cn",
    "zai",
    "zai-coding-plan",
    "perplexity",
    "nvidia",
    "togetherai",
    "fireworks-ai",
    "amazon-bedrock",
    "azure",
];

pub struct UsageService {
    data_home: RwLock<PathBuf>,
    database: UsageDatabase,
    operation_lock: Mutex<()>,
    watch_sender: Mutex<Option<Sender<WatchSignal>>>,
    weekly_usage_cache: Mutex<Option<CachedWeeklyUsage>>,
}

struct CachedWeeklyUsage {
    data_home: PathBuf,
    fetched_at: Instant,
    value: Option<WeeklyUsage>,
}

#[derive(Clone, Copy)]
enum WatchSignal {
    FilesChanged,
    Reconfigure,
}

impl UsageService {
    pub fn open_default() -> Result<Self, String> {
        let project_dirs = app_project_dirs()?;
        let default_home = resolve_codex_home()?;
        let data_home = if std::env::var_os("CODEX_HOME").is_some() {
            default_home
        } else {
            load_codex_home_preference()
                .filter(|path| path.is_dir())
                .unwrap_or(default_home)
        };
        let database_path = project_dirs.data_local_dir().join("usage.sqlite3");
        Self::open(data_home, database_path)
    }

    pub fn open(data_home: PathBuf, database_path: PathBuf) -> Result<Self, String> {
        Ok(Self {
            data_home: RwLock::new(data_home),
            database: UsageDatabase::open(&database_path)?,
            operation_lock: Mutex::new(()),
            watch_sender: Mutex::new(None),
            weekly_usage_cache: Mutex::new(None),
        })
    }

    pub fn codex_home(&self) -> PathBuf {
        self.data_home
            .read()
            .map(|path| path.clone())
            .unwrap_or_default()
    }

    pub fn source_kind(&self) -> DataSourceKind {
        detect_data_source(&self.codex_home())
    }

    pub fn set_codex_home(&self, path: PathBuf) -> Result<(), String> {
        if !path.is_dir() {
            return Err(format!("所选 Home 不存在: {}", path.display()));
        }
        {
            let _operation = self
                .operation_lock
                .lock()
                .map_err(|error| error.to_string())?;
            self.database.clear_usage()?;
            *self.data_home.write().map_err(|error| error.to_string())? = path;
        }
        if let Some(sender) = self
            .watch_sender
            .lock()
            .map_err(|error| error.to_string())?
            .as_ref()
        {
            let _ = sender.send(WatchSignal::Reconfigure);
        }
        Ok(())
    }

    pub fn sync(&self) -> Result<SyncResult, String> {
        let _operation = self
            .operation_lock
            .lock()
            .map_err(|error| error.to_string())?;
        let data_home = self.codex_home();
        let source_kind = self.source_kind();
        let mut result = SyncResult::default();
        for path in collect_session_files(&data_home, source_kind) {
            result.scanned_files += 1;
            let key = source_key(&path);
            let cursor = match self.database.load_cursor(&key) {
                Ok(cursor) => cursor,
                Err(error) => {
                    result
                        .warnings
                        .push(format!("读取游标失败 {}: {error}", path.display()));
                    continue;
                }
            };
            let previous_cursor = cursor.clone();
            let outcome = match parse_session_file_for_source(&path, cursor, source_kind) {
                Ok(outcome) => outcome,
                Err(error) => {
                    result.warnings.push(error);
                    continue;
                }
            };
            if outcome.reset_required {
                result.rebuilt_files += 1;
            }
            if outcome.events.is_empty()
                && !outcome.reset_required
                && previous_cursor.as_ref() == Some(&outcome.cursor)
            {
                continue;
            }
            result.changed_files += 1;
            match self.database.apply_parse_outcome(&outcome) {
                Ok(inserted) => result.imported_events += inserted,
                Err(error) => result
                    .warnings
                    .push(format!("写入统计失败 {}: {error}", path.display())),
            }
        }
        Ok(result)
    }

    pub fn reset_usage_cache(&self) -> Result<SyncResult, String> {
        let operation = self
            .operation_lock
            .lock()
            .map_err(|error| error.to_string())?;
        self.database.clear_usage()?;
        drop(operation);
        self.sync()
    }

    pub fn snapshot(&self, range: UsageRange) -> Result<UsageSnapshot, String> {
        let source_kind = self.source_kind();
        self.database
            .snapshot(&self.codex_home(), source_kind, range)
    }

    pub fn account_weekly_usage(&self) -> Option<WeeklyUsage> {
        if self.source_kind() == DataSourceKind::ClaudeCode {
            return None;
        }
        let data_home = self.codex_home();
        let mut cache = self.weekly_usage_cache.lock().ok()?;
        if let Some(cached) = cache.as_ref() {
            if cached.data_home == data_home && cached.fetched_at.elapsed() < WEEKLY_USAGE_CACHE_TTL
            {
                return cached.value.clone();
            }
        }

        let value = query_account_weekly_usage(&data_home);
        *cache = Some(CachedWeeklyUsage {
            data_home,
            fetched_at: Instant::now(),
            value: value.clone(),
        });
        value
    }

    pub fn used_model_prices(&self) -> Result<Vec<ModelPriceEntry>, String> {
        self.database.used_model_prices()
    }

    pub fn update_model_price(
        &self,
        model: &str,
        input_per_million: &str,
        cached_input_per_million: &str,
        output_per_million: &str,
        multiplier: &str,
    ) -> Result<(), String> {
        self.database.update_model_price(
            model,
            input_per_million,
            cached_input_per_million,
            output_per_million,
            multiplier,
        )
    }

    pub fn refresh_models_dev_prices(&self) -> Result<u64, String> {
        let response = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(12))
            .build()
            .map_err(|error| error.to_string())?
            .get("https://models.dev/api.json")
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|error| format!("无法读取 models.dev: {error}"))?
            .json::<serde_json::Value>()
            .map_err(|error| format!("无法解析 models.dev: {error}"))?;
        let mut prices = Vec::new();
        for provider in MODELS_DEV_PROVIDERS {
            let Some(models) = response
                .get(provider)
                .and_then(|value| value.get("models"))
                .and_then(serde_json::Value::as_object)
            else {
                continue;
            };
            for (model, value) in models {
                let Some(input) = catalog_number(value.pointer("/cost/input")) else {
                    continue;
                };
                let Some(output) = catalog_number(value.pointer("/cost/output")) else {
                    continue;
                };
                let cached = catalog_number(value.pointer("/cost/cache_read"))
                    .unwrap_or_else(|| input.clone());
                let display_name = value
                    .get("name")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(model)
                    .to_string();
                prices.push(CatalogModelPrice {
                    model: model.to_string(),
                    display_name,
                    input_per_million: input,
                    cached_input_per_million: cached,
                    output_per_million: output,
                });
            }
        }
        if prices.is_empty() {
            return Err("models.dev 未返回可用模型目录".to_string());
        }
        self.database.import_catalog_prices(&prices)
    }

    pub fn start_background_sync(
        self: &Arc<Self>,
        on_update: impl Fn() + Send + Sync + 'static,
    ) -> Result<(), String> {
        let service = Arc::clone(self);
        let callback = Arc::new(on_update);
        let (sender, receiver) = std::sync::mpsc::channel::<WatchSignal>();
        let event_sender = sender.clone();
        let watched_home = self.codex_home();
        let mut watcher = create_session_watcher(&watched_home, event_sender.clone())?;
        if watched_home.exists() {
            watcher
                .watch(&watched_home, RecursiveMode::Recursive)
                .map_err(|error| format!("无法监控 {}: {error}", watched_home.display()))?;
        }
        *self
            .watch_sender
            .lock()
            .map_err(|error| error.to_string())? = Some(sender);
        std::thread::spawn(move || {
            while let Ok(signal) = receiver.recv() {
                let mut files_changed = matches!(signal, WatchSignal::FilesChanged);
                let mut reconfigure = matches!(signal, WatchSignal::Reconfigure);
                if files_changed {
                    std::thread::sleep(Duration::from_millis(300));
                }
                while let Ok(signal) = receiver.try_recv() {
                    files_changed |= matches!(signal, WatchSignal::FilesChanged);
                    reconfigure |= matches!(signal, WatchSignal::Reconfigure);
                }
                if reconfigure {
                    let replacement_home = service.codex_home();
                    if let Ok(mut replacement) =
                        create_session_watcher(&replacement_home, event_sender.clone())
                    {
                        if replacement_home.exists() {
                            let _ = replacement.watch(&replacement_home, RecursiveMode::Recursive);
                        }
                        watcher = replacement;
                    }
                }
                if files_changed {
                    if let Ok(result) = service.sync() {
                        if result.imported_events > 0 || result.rebuilt_files > 0 {
                            callback();
                        }
                    }
                }
                let _watcher_guard = &watcher;
            }
        });
        Ok(())
    }
}

fn query_account_weekly_usage(data_home: &Path) -> Option<WeeklyUsage> {
    let mut child = start_codex_app_server(data_home)?;
    let (Some(mut stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        terminate_app_server(&mut child);
        return None;
    };
    let requests = [
        json!({
            "method": "initialize",
            "id": 1,
            "params": {
                "clientInfo": {
                    "name": "quota_loom",
                    "title": "QuotaLoom",
                    "version": env!("CARGO_PKG_VERSION")
                }
            }
        }),
        json!({ "method": "initialized" }),
        json!({
            "method": "account/read",
            "id": 2,
            "params": { "refreshToken": false }
        }),
        json!({ "method": "account/rateLimits/read", "id": 3 }),
    ];
    for request in requests {
        if serde_json::to_writer(&mut stdin, &request).is_err() || stdin.write_all(b"\n").is_err() {
            drop(stdin);
            terminate_app_server(&mut child);
            return None;
        }
    }
    if stdin.flush().is_err() {
        drop(stdin);
        terminate_app_server(&mut child);
        return None;
    }

    let (sender, receiver) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                continue;
            };
            if matches!(message.get("id").and_then(Value::as_i64), Some(2 | 3))
                && sender.send(message).is_err()
            {
                break;
            }
        }
    });

    let deadline = Instant::now() + APP_SERVER_TIMEOUT;
    let mut chatgpt_account = None;
    let mut rate_limits = None;
    let mut rate_limits_received = false;
    while Instant::now() < deadline
        && chatgpt_account != Some(false)
        && (chatgpt_account.is_none() || !rate_limits_received)
    {
        let timeout = deadline.saturating_duration_since(Instant::now());
        let Ok(message) = receiver.recv_timeout(timeout) else {
            break;
        };
        match message.get("id").and_then(Value::as_i64) {
            Some(2) => {
                chatgpt_account = Some(
                    message
                        .pointer("/result/account/type")
                        .and_then(Value::as_str)
                        == Some("chatgpt"),
                );
            }
            Some(3) => {
                rate_limits_received = true;
                rate_limits = message.get("result").cloned();
            }
            _ => {}
        }
    }

    drop(stdin);
    terminate_app_server(&mut child);
    let _ = reader.join();

    if chatgpt_account != Some(true) {
        return None;
    }
    parse_weekly_usage(&rate_limits?)
}

fn terminate_app_server(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

fn start_codex_app_server(data_home: &Path) -> Option<Child> {
    for binary in codex_binary_candidates() {
        let child = Command::new(binary)
            .args(["app-server", "--stdio"])
            .env("CODEX_HOME", data_home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        if let Ok(child) = child {
            return Some(child);
        }
    }
    None
}

fn codex_binary_candidates() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(binary) = std::env::var_os("CODEX_BINARY").filter(|value| !value.is_empty()) {
        candidates.push(PathBuf::from(binary));
    }
    candidates.push(PathBuf::from("codex"));
    candidates.extend([
        PathBuf::from("/opt/homebrew/bin/codex"),
        PathBuf::from("/usr/local/bin/codex"),
        PathBuf::from("/usr/bin/codex"),
    ]);
    if let Some(user_dirs) = UserDirs::new() {
        candidates.push(user_dirs.home_dir().join(".local/bin/codex"));
        candidates.push(user_dirs.home_dir().join(".npm-global/bin/codex"));
    }
    candidates
}

fn parse_weekly_usage(response: &Value) -> Option<WeeklyUsage> {
    let mut snapshots = Vec::new();
    if let Some(snapshot) = response.pointer("/rateLimitsByLimitId/codex") {
        snapshots.push(snapshot);
    }
    if let Some(snapshot) = response.get("rateLimits") {
        snapshots.push(snapshot);
    }
    if let Some(by_id) = response
        .get("rateLimitsByLimitId")
        .and_then(Value::as_object)
    {
        snapshots.extend(by_id.values());
    }

    for snapshot in snapshots {
        for key in ["primary", "secondary"] {
            let Some(window) = snapshot.get(key) else {
                continue;
            };
            if window.get("windowDurationMins").and_then(Value::as_i64)
                != Some(WEEKLY_WINDOW_MINUTES)
            {
                continue;
            }
            let used_percent = window.get("usedPercent")?.as_f64()?.clamp(0.0, 100.0);
            return Some(WeeklyUsage {
                used_percent,
                remaining_percent: 100.0 - used_percent,
                resets_at: window.get("resetsAt").and_then(Value::as_i64),
            });
        }
    }
    None
}

fn create_session_watcher(
    codex_home: &Path,
    sender: Sender<WatchSignal>,
) -> Result<Box<dyn Watcher + Send>, String> {
    let handler = move |event: notify::Result<notify::Event>| {
        if event.is_ok() {
            let _ = sender.send(WatchSignal::FilesChanged);
        }
    };
    if requires_polling_watcher(codex_home) {
        PollWatcher::new(
            handler,
            notify::Config::default()
                .with_poll_interval(Duration::from_secs(1))
                .with_compare_contents(false),
        )
        .map(|watcher| Box::new(watcher) as Box<dyn Watcher + Send>)
        .map_err(|error| error.to_string())
    } else {
        RecommendedWatcher::new(handler, notify::Config::default())
            .map(|watcher| Box::new(watcher) as Box<dyn Watcher + Send>)
            .map_err(|error| error.to_string())
    }
}

fn requires_polling_watcher(path: &Path) -> bool {
    cfg!(target_os = "windows") && path.to_string_lossy().starts_with(r"\\")
}

fn catalog_number(value: Option<&serde_json::Value>) -> Option<String> {
    value?.as_number().map(ToString::to_string)
}

pub fn resolve_codex_home() -> Result<PathBuf, String> {
    if let Some(value) = std::env::var_os("CODEX_HOME").filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(value));
    }
    let user_dirs = UserDirs::new().ok_or_else(|| "无法确定用户主目录".to_string())?;
    Ok(user_dirs.home_dir().join(".codex"))
}

pub fn app_project_dirs() -> Result<ProjectDirs, String> {
    ProjectDirs::from("io.github", "ericsmoon", "QuotaLoom")
        .ok_or_else(|| "无法确定应用数据目录".to_string())
}

pub fn save_codex_home_preference(path: &Path) -> Result<(), String> {
    let preference_path = codex_home_preference_path()?;
    if let Some(parent) = preference_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        preference_path,
        serde_json::to_vec(&path.to_string_lossy().to_string())
            .map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

fn load_codex_home_preference() -> Option<PathBuf> {
    let data = std::fs::read(codex_home_preference_path().ok()?).ok()?;
    serde_json::from_slice::<String>(&data)
        .ok()
        .map(PathBuf::from)
}

fn codex_home_preference_path() -> Result<PathBuf, String> {
    Ok(app_project_dirs()?
        .config_local_dir()
        .join("codex-home.json"))
}

fn session_roots(data_home: &Path, source_kind: DataSourceKind) -> Vec<PathBuf> {
    match source_kind {
        DataSourceKind::ClaudeCode => vec![data_home.join("projects")],
        DataSourceKind::CodexCli | DataSourceKind::ChatGptCodex => vec![
            data_home.join("sessions"),
            data_home.join("archived_sessions"),
        ],
    }
}

fn collect_session_files(data_home: &Path, source_kind: DataSourceKind) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for root in session_roots(data_home, source_kind) {
        if !root.exists() {
            continue;
        }
        for entry in WalkDir::new(root)
            .follow_links(false)
            .max_depth(12)
            .into_iter()
            .filter_map(Result::ok)
        {
            if entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    == Some("jsonl")
            {
                files.push(entry.into_path());
            }
        }
    }
    files.sort();
    files
}

fn detect_data_source(path: &Path) -> DataSourceKind {
    let name = path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    if name == ".claude"
        || name == "claude"
        || path.join("projects").is_dir()
        || path.join("file-history").is_dir()
    {
        return DataSourceKind::ClaudeCode;
    }
    if name == ".chatgpt"
        || name == "chatgpt"
        || name.contains("chatgpt")
        || path.join("chatgpt").is_dir()
    {
        return DataSourceKind::ChatGptCodex;
    }
    if path.join("sessions").is_dir() && contains_chatgpt_session_metadata(path) {
        return DataSourceKind::ChatGptCodex;
    }
    DataSourceKind::CodexCli
}

fn contains_chatgpt_session_metadata(data_home: &Path) -> bool {
    for entry in WalkDir::new(data_home.join("sessions"))
        .follow_links(false)
        .max_depth(5)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry.file_type().is_file()
                && entry
                    .path()
                    .extension()
                    .and_then(|extension| extension.to_str())
                    == Some("jsonl")
        })
        .take(8)
    {
        let Ok(content) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        for line in content.lines().filter(|line| line.contains("session_meta")) {
            let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
                continue;
            };
            let payload = value.get("payload").unwrap_or(&value);
            let originator = payload
                .get("originator")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            let source = payload
                .get("source")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_ascii_lowercase();
            if originator.contains("chatgpt")
                || originator.contains("codex-app")
                || source == "app"
                || source == "desktop"
            {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use tempfile::TempDir;

    fn line(value: Value) -> String {
        format!("{}\n", serde_json::to_string(&value).unwrap())
    }

    fn token_line(timestamp: &str, input: u64, cached: u64, output: u64) -> String {
        line(json!({
            "type": "event_msg",
            "timestamp": timestamp,
            "payload": {
                "type": "token_count",
                "info": {
                    "total_token_usage": {
                        "input_tokens": input,
                        "cached_input_tokens": cached,
                        "output_tokens": output
                    }
                }
            }
        }))
    }

    fn all_time() -> UsageRange {
        UsageRange {
            start_at: 0,
            end_at: i64::MAX,
            bucket_seconds: 3600,
            timezone_offset_seconds: 0,
        }
    }

    #[test]
    fn reads_weekly_window_from_either_rate_limit_slot() {
        let primary = parse_weekly_usage(&json!({
            "rateLimits": {
                "primary": {
                    "usedPercent": 12.5,
                    "windowDurationMins": 10080,
                    "resetsAt": 1_800_000_000
                },
                "secondary": null
            }
        }))
        .unwrap();
        assert_eq!(primary.used_percent, 12.5);
        assert_eq!(primary.remaining_percent, 87.5);
        assert_eq!(primary.resets_at, Some(1_800_000_000));

        let secondary = parse_weekly_usage(&json!({
            "rateLimits": {
                "primary": {
                    "usedPercent": 40,
                    "windowDurationMins": 300,
                    "resetsAt": 1_700_000_000
                },
                "secondary": {
                    "usedPercent": 21,
                    "windowDurationMins": 10080,
                    "resetsAt": 1_800_000_001
                }
            }
        }))
        .unwrap();
        assert_eq!(secondary.remaining_percent, 79.0);
        assert_eq!(secondary.resets_at, Some(1_800_000_001));
    }

    #[test]
    fn hides_rate_limits_without_a_weekly_window() {
        let usage = parse_weekly_usage(&json!({
            "rateLimits": {
                "primary": {
                    "usedPercent": 40,
                    "windowDurationMins": 300,
                    "resetsAt": 1_700_000_000
                },
                "secondary": null
            }
        }));
        assert_eq!(usage, None);
    }

    #[test]
    fn syncs_appends_without_duplicates_and_rebuilds_truncated_files() {
        let temp = TempDir::new().unwrap();
        let codex_home = temp.path().join("codex");
        let sessions = codex_home.join("sessions/2026/07/14");
        fs::create_dir_all(&sessions).unwrap();
        let session_path = sessions.join("rollout-thread-a.jsonl");
        let initial = format!(
            "{}{}{}",
            line(json!({"type":"session_meta","payload":{"id":"thread-a"}})),
            line(json!({"type":"turn_context","payload":{"model":"gpt-5.3-codex"}})),
            token_line("2026-07-14T08:00:00Z", 100, 40, 20),
        );
        fs::write(&session_path, initial).unwrap();

        let service = UsageService::open(codex_home, temp.path().join("usage.sqlite3")).unwrap();
        let first = service.sync().unwrap();
        assert_eq!(first.imported_events, 1);
        assert_eq!(
            service.snapshot(all_time()).unwrap().summary.total_tokens,
            120
        );

        let mut file = OpenOptions::new().append(true).open(&session_path).unwrap();
        file.write_all(token_line("2026-07-14T08:01:00Z", 250, 90, 55).as_bytes())
            .unwrap();
        file.flush().unwrap();

        let second = service.sync().unwrap();
        assert_eq!(second.imported_events, 1);
        let after_append = service.snapshot(all_time()).unwrap();
        assert_eq!(after_append.summary.calls, 2);
        assert_eq!(after_append.summary.total_tokens, 305);
        assert_eq!(after_append.summary.fresh_input_tokens, 160);
        assert_eq!(after_append.summary.cached_input_tokens, 90);
        assert_eq!(after_append.summary.output_tokens, 55);

        let unchanged = service.sync().unwrap();
        assert_eq!(unchanged.changed_files, 0);
        assert_eq!(unchanged.imported_events, 0);
        assert_eq!(service.snapshot(all_time()).unwrap().summary.calls, 2);

        let replacement = format!(
            "{}{}{}",
            line(json!({"type":"session_meta","payload":{"id":"thread-a"}})),
            line(json!({"type":"turn_context","payload":{"model":"gpt-5.3-codex"}})),
            token_line("2026-07-14T09:00:00Z", 50, 10, 5),
        );
        fs::write(&session_path, replacement).unwrap();

        let rebuilt = service.sync().unwrap();
        assert_eq!(rebuilt.rebuilt_files, 1);
        assert_eq!(rebuilt.imported_events, 1);
        let after_rebuild = service.snapshot(all_time()).unwrap();
        assert_eq!(after_rebuild.summary.calls, 1);
        assert_eq!(after_rebuild.summary.total_tokens, 55);
        assert_eq!(after_rebuild.summary.fresh_input_tokens, 40);
        assert_eq!(after_rebuild.summary.cached_input_tokens, 10);
        assert_eq!(after_rebuild.summary.output_tokens, 5);
    }
}
