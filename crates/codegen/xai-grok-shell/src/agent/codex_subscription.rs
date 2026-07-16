//! Cooperative reuse of Codex's ChatGPT subscription credentials.
//!
//! The refresh path is intentionally startup-only. It serializes ginf refreshes,
//! re-reads `auth.json` before persisting, and never replaces credentials that
//! another Codex-family process rotated while the request was in flight.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{Duration, Utc};
use fs2::FileExt;
use serde_json::Value;

pub const MODEL_ID: &str = "openai-max";
pub const MODEL_SLUG: &str = "gpt-5.6-sol";
pub const BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const REFRESH_URL: &str = "https://auth.openai.com/oauth/token";
const REFRESH_MARGIN_MINUTES: i64 = 5;

static READY: AtomicBool = AtomicBool::new(false);
static EXTENDER_STARTED: AtomicBool = AtomicBool::new(false);

#[derive(Clone, Debug)]
pub struct Credentials {
    pub access_token: String,
    pub account_id: String,
}

#[derive(Debug)]
pub struct CodexBearerResolver;

impl xai_grok_sampler::BearerResolver for CodexBearerResolver {
    fn current_bearer(&self) -> Option<String> {
        load_credentials().map(|credentials| credentials.access_token)
    }
}

pub fn is_ready() -> bool {
    READY.load(Ordering::Acquire)
}

pub fn bearer_resolver() -> xai_grok_sampler::SharedBearerResolver {
    std::sync::Arc::new(CodexBearerResolver)
}

pub fn load_credentials() -> Option<Credentials> {
    let env_token = nonempty_env("CODEX_ACCESS_TOKEN");
    let env_account = nonempty_env("CHATGPT_ACCOUNT_ID");
    if let (Some(access_token), Some(account_id)) = (env_token, env_account.clone()) {
        return Some(Credentials {
            access_token,
            account_id,
        });
    }

    let value = read_auth_json(&auth_json_path()?)?;
    credentials_from_value(&value, env_account)
}

/// Prepare the shared Codex login once at ginf startup.
///
/// A valid cached token is left untouched. An expired/near-expiry file token is
/// refreshed once, under a ginf lock. A compare-before-write guard prevents a
/// concurrent Codex/Codex Infinity refresh from being overwritten.
pub async fn prepare() -> bool {
    if let Some(credentials) = env_credentials() {
        let ready = token_is_usable(&credentials.access_token);
        READY.store(ready, Ordering::Release);
        return ready;
    }

    let Some(auth_path) = auth_json_path() else {
        return false;
    };
    let Some(initial) = read_auth_json(&auth_path) else {
        return false;
    };
    if credentials_from_value(&initial, None)
        .is_some_and(|credentials| token_is_usable(&credentials.access_token))
    {
        READY.store(true, Ordering::Release);
        start_session_extender(auth_path);
        return true;
    }

    let ready = refresh_file_credentials(&auth_path).await;
    READY.store(ready, Ordering::Release);
    if ready {
        start_session_extender(auth_path);
    }
    ready
}

/// Keep a long-running ginf process authenticated without owning the login.
///
/// Each tick normally only reads `auth.json`. Refresh happens only inside the
/// expiry margin and retains the same lock + compare-before-write protections
/// used during startup. Rotated refresh tokens are therefore consumed once.
fn start_session_extender(auth_path: PathBuf) {
    if EXTENDER_STARTED.swap(true, Ordering::AcqRel) {
        return;
    }
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        interval.tick().await;
        loop {
            interval.tick().await;
            let usable = disk_now_has_usable_credentials(&auth_path)
                || refresh_file_credentials(&auth_path).await;
            READY.store(usable, Ordering::Release);
        }
    });
}

fn env_credentials() -> Option<Credentials> {
    Some(Credentials {
        access_token: nonempty_env("CODEX_ACCESS_TOKEN")?,
        account_id: nonempty_env("CHATGPT_ACCOUNT_ID")?,
    })
}

fn nonempty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn auth_json_path() -> Option<PathBuf> {
    let codex_home = std::env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".codex")))?;
    Some(codex_home.join("auth.json"))
}

fn read_auth_json(path: &Path) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

fn credentials_from_value(value: &Value, account_override: Option<String>) -> Option<Credentials> {
    let tokens = value.get("tokens")?;
    let access_token = tokens
        .get("access_token")?
        .as_str()
        .filter(|value| !value.trim().is_empty())?
        .to_string();
    let account_id = account_override.or_else(|| {
        tokens
            .get("account_id")?
            .as_str()
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
    })?;
    Some(Credentials {
        access_token,
        account_id,
    })
}

fn token_is_usable(token: &str) -> bool {
    !crate::auth::is_jwt_expired_or_near(token, Duration::minutes(REFRESH_MARGIN_MINUTES))
}

async fn refresh_file_credentials(auth_path: &Path) -> bool {
    let Some(parent) = auth_path.parent() else {
        return false;
    };
    let lock_path = parent.join(".ginf-auth-refresh.lock");
    let Ok(lock) = OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .open(lock_path)
    else {
        return false;
    };
    if lock.lock_exclusive().is_err() {
        return false;
    }

    let Some(mut before) = read_auth_json(auth_path) else {
        return false;
    };
    if credentials_from_value(&before, None)
        .is_some_and(|credentials| token_is_usable(&credentials.access_token))
    {
        return true;
    }
    let Some(refresh_token) = before
        .pointer("/tokens/refresh_token")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
    else {
        return false;
    };

    let response = reqwest::Client::new()
        .post(REFRESH_URL)
        .json(&serde_json::json!({
            "client_id": CLIENT_ID,
            "grant_type": "refresh_token",
            "refresh_token": refresh_token,
        }))
        .send()
        .await;

    let Ok(response) = response else {
        return disk_now_has_usable_credentials(auth_path);
    };
    if !response.status().is_success() {
        return disk_now_has_usable_credentials(auth_path);
    }
    let Ok(refreshed) = response.json::<Value>().await else {
        return disk_now_has_usable_credentials(auth_path);
    };

    // Codex may have refreshed while our HTTP request was running. Its rotated
    // refresh token wins; never write our now-stale response over it.
    if let Some(current) = read_auth_json(auth_path) {
        let current_refresh = current
            .pointer("/tokens/refresh_token")
            .and_then(Value::as_str);
        if current_refresh != Some(refresh_token.as_str()) {
            return credentials_from_value(&current, None)
                .is_some_and(|credentials| token_is_usable(&credentials.access_token));
        }
        before = current;
    }

    let Some(tokens) = before.get_mut("tokens").and_then(Value::as_object_mut) else {
        return false;
    };
    let Some(access_token) = refreshed
        .get("access_token")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
    else {
        return false;
    };
    tokens.insert(
        "access_token".to_string(),
        Value::String(access_token.to_string()),
    );
    for key in ["refresh_token", "id_token"] {
        if let Some(value) = refreshed
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
        {
            tokens.insert(key.to_string(), Value::String(value.to_string()));
        }
    }
    before["last_refresh"] = Value::String(Utc::now().to_rfc3339());

    write_auth_json_atomically(auth_path, &before).is_ok()
        && credentials_from_value(&before, None)
            .is_some_and(|credentials| token_is_usable(&credentials.access_token))
}

fn disk_now_has_usable_credentials(path: &Path) -> bool {
    read_auth_json(path)
        .and_then(|value| credentials_from_value(&value, None))
        .is_some_and(|credentials| token_is_usable(&credentials.access_token))
}

fn write_auth_json_atomically(path: &Path, value: &Value) -> std::io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("auth.json has no parent"))?;
    let temp = parent.join(format!(".auth.json.ginf-{}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temp)?;
    serde_json::to_writer_pretty(&mut file, value).map_err(std::io::Error::other)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    std::fs::rename(&temp, path)?;
    sync_parent(parent);
    Ok(())
}

fn sync_parent(parent: &Path) {
    if let Ok(directory) = File::open(parent) {
        let _ = directory.sync_all();
    }
}
