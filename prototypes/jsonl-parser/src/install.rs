use anyhow::{Context, Result, bail};
use serde_json::{json, Value};
use std::path::PathBuf;
use tracing::warn;

// 4 hook events T1 theo REQ §4.3.2-3
const HOOK_EVENTS: &[(&str, &str)] = &[
    ("SessionStart",      "session-start"),
    ("UserPromptSubmit",  "user-prompt-submit"),
    ("Stop",              "stop"),
    ("SessionEnd",        "session-end"),
];

const MARKER: &str = "_c2_tracker";

// Đọc token từ ~/.c2-tracker/runtime.json
fn read_runtime_token() -> Result<String> {
    let rt_path = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".c2-tracker")
        .join("runtime.json");

    if !rt_path.exists() {
        bail!(
            "Chưa tìm thấy ~/.c2-tracker/runtime.json.\n\
             Chạy `c2-engine serve` ít nhất 1 lần trước để sinh token."
        );
    }

    let raw = std::fs::read_to_string(&rt_path)
        .with_context(|| format!("Đọc {}", rt_path.display()))?;
    let v: Value = serde_json::from_str(&raw)
        .with_context(|| "Parse runtime.json")?;

    v["token"]
        .as_str()
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow::anyhow!("runtime.json thiếu field 'token'"))
}

/// Chạy `claude --version`, parse semver x.y.z
pub fn detect_claude_version() -> Option<(u32, u32, u32)> {
    let output = std::process::Command::new("claude")
        .arg("--version")
        .output()
        .ok()?;

    let text = String::from_utf8_lossy(&output.stdout);
    // "Claude Code 2.1.143" hoặc "2.1.143"
    for word in text.split_whitespace() {
        let parts: Vec<&str> = word.split('.').collect();
        if parts.len() == 3 {
            if let (Ok(major), Ok(minor), Ok(patch)) = (
                parts[0].parse::<u32>(),
                parts[1].parse::<u32>(),
                parts[2].parse::<u32>(),
            ) {
                return Some((major, minor, patch));
            }
        }
    }
    None
}

/// Sinh http hook entry (Claude Code >= 2.0)
pub fn build_http_hook(port: u16, endpoint: &str) -> Value {
    let mut v = json!({
        "type": "http",
        "url": format!("http://127.0.0.1:{}/hooks/{}", port, endpoint),
        "headers": { "Authorization": "Bearer ${CT_TOKEN}" },
        "allowedEnvVars": ["CT_TOKEN"],
        "timeout": 5
    });
    v[MARKER] = json!(true);
    v
}

/// Sinh command shim hook (fallback cho Claude Code < 2.0)
pub fn build_command_hook(port: u16, endpoint: &str) -> Value {
    let cmd = format!(
        "source ~/.c2-tracker/env && curl -s -X POST \
         -H \"Authorization: Bearer $CT_TOKEN\" \
         -H 'Content-Type: application/json' \
         --max-time 5 http://127.0.0.1:{}/hooks/{} -d @-",
        port, endpoint
    );
    let mut v = json!({
        "type": "command",
        "command": cmd
    });
    v[MARKER] = json!(true);
    v
}

fn default_settings_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".claude")
        .join("settings.json")
}

fn backup_settings(settings_path: &PathBuf) -> Result<()> {
    if !settings_path.exists() {
        return Ok(());
    }
    let ts = chrono::Utc::now().timestamp();
    let bak = settings_path.with_extension(format!("json.bak.{}", ts));
    std::fs::copy(settings_path, &bak)
        .with_context(|| format!("Backup -> {}", bak.display()))?;
    println!("Backup: {}", bak.display());
    Ok(())
}

/// Kiểm tra xem trong hooks có entry nào có _c2_tracker: true chưa
fn has_c2_tracker_entry(hooks_obj: &Value) -> bool {
    let Some(obj) = hooks_obj.as_object() else { return false; };
    for event_val in obj.values() {
        let Some(arr) = event_val.as_array() else { continue; };
        for matcher in arr {
            let Some(inner) = matcher.get("hooks").and_then(|h| h.as_array()) else { continue; };
            for hook in inner {
                if hook.get(MARKER).and_then(|v| v.as_bool()) == Some(true) {
                    return true;
                }
            }
        }
    }
    false
}

/// Remove tất cả entry có _c2_tracker: true khỏi hooks object
fn remove_c2_tracker_entries(hooks_obj: &mut Value) {
    let Some(obj) = hooks_obj.as_object_mut() else { return; };
    for event_val in obj.values_mut() {
        let Some(matchers) = event_val.as_array_mut() else { continue; };
        for matcher in matchers.iter_mut() {
            let Some(inner) = matcher.get_mut("hooks").and_then(|h| h.as_array_mut()) else { continue; };
            inner.retain(|hook| {
                hook.get(MARKER).and_then(|v| v.as_bool()) != Some(true)
            });
        }
        // Xoá matcher rỗng
        matchers.retain(|matcher| {
            matcher.get("hooks")
                .and_then(|h| h.as_array())
                .map(|arr| !arr.is_empty())
                .unwrap_or(true)
        });
    }
}

/// Ghi token vào ~/.c2-tracker/env (chmod 600)
fn write_env_file(token: &str) -> Result<PathBuf> {
    let dir = dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".c2-tracker");
    std::fs::create_dir_all(&dir)
        .with_context(|| format!("Tạo {}", dir.display()))?;

    let env_path = dir.join("env");
    std::fs::write(&env_path, format!("CT_TOKEN={}\n", token))
        .with_context(|| format!("Ghi {}", env_path.display()))?;

    // chmod 600
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&env_path, std::fs::Permissions::from_mode(0o600))
            .with_context(|| "chmod 600 env file")?;
    }

    Ok(env_path)
}

/// Cài hooks vào settings.json
pub fn install(
    settings: Option<PathBuf>,
    port: u16,
    force: bool,
    force_command_shim: bool,
) -> Result<()> {
    let token = read_runtime_token()?;
    let settings_path = settings.unwrap_or_else(default_settings_path);

    // Detect version để chọn hook type
    let use_command_shim = if force_command_shim {
        println!("--force-command-shim: dùng curl shim.");
        true
    } else {
        match detect_claude_version() {
            Some((major, _, _)) if major >= 2 => {
                println!("Phát hiện Claude Code {}.x → dùng http hook native.", major);
                false
            }
            Some((major, minor, patch)) => {
                warn!(
                    "Claude Code {}.{}.{} < 2.0 — fallback sang command shim (curl). \
                     Khuyến nghị nâng cấp lên >= 2.0.",
                    major, minor, patch
                );
                true
            }
            None => {
                warn!(
                    "Không phát hiện được Claude Code (`claude --version` thất bại). \
                     Dùng http hook (sẽ hoạt động khi Claude Code 2.x được cài)."
                );
                false
            }
        }
    };

    // Đọc settings.json hiện tại (hoặc tạo mới)
    let mut root: Value = if settings_path.exists() {
        let raw = std::fs::read_to_string(&settings_path)
            .with_context(|| format!("Đọc {}", settings_path.display()))?;
        serde_json::from_str(&raw).unwrap_or_else(|_| json!({}))
    } else {
        json!({})
    };

    // Kiểm tra idempotency
    let hooks_val = root.get("hooks").cloned().unwrap_or_else(|| json!({}));
    if has_c2_tracker_entry(&hooks_val) {
        if !force {
            println!(
                "Hooks C2-Tracker đã được cài. Dùng --force để cài lại."
            );
            return Ok(());
        }
        println!("--force: xoá hooks cũ rồi cài lại.");
    }

    backup_settings(&settings_path)?;

    // Build hooks object mới
    let hooks_obj = root
        .as_object_mut()
        .unwrap()
        .entry("hooks")
        .or_insert_with(|| json!({}));

    // Remove entries cũ nếu force
    if force {
        remove_c2_tracker_entries(hooks_obj);
    }

    // Thêm 4 hook T1
    for (event_name, endpoint) in HOOK_EVENTS {
        let hook_entry = if use_command_shim {
            build_command_hook(port, endpoint)
        } else {
            build_http_hook(port, endpoint)
        };

        let matcher = json!({ "hooks": [hook_entry] });

        let event_arr = hooks_obj
            .as_object_mut()
            .unwrap()
            .entry(*event_name)
            .or_insert_with(|| json!([]));

        event_arr
            .as_array_mut()
            .unwrap()
            .push(matcher);
    }

    // Tạo thư mục nếu cần
    if let Some(parent) = settings_path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Tạo {}", parent.display()))?;
    }

    // Ghi settings.json
    let out = serde_json::to_string_pretty(&root)?;
    std::fs::write(&settings_path, out)
        .with_context(|| format!("Ghi {}", settings_path.display()))?;

    // Ghi env file
    let env_path = write_env_file(&token)?;

    // Print hướng dẫn
    println!();
    println!("Hooks cai vao {}", settings_path.display());
    println!("Token luu tai {}", env_path.display());
    println!();
    println!("LUU Y: de hooks hoat dong, Claude Code phai doc duoc bien CT_TOKEN.");
    println!("Them vao shell init (~/.zshrc):");
    println!("    source ~/.c2-tracker/env");
    println!();
    println!("Hoac khoi dong Claude Code voi:");
    println!(
        "    CT_TOKEN=$(grep CT_TOKEN ~/.c2-tracker/env | cut -d= -f2) claude"
    );

    Ok(())
}

/// Gỡ hooks C2-Tracker khỏi settings.json
pub fn uninstall(settings: Option<PathBuf>) -> Result<()> {
    let settings_path = settings.unwrap_or_else(default_settings_path);

    if !settings_path.exists() {
        println!("Không tìm thấy {}", settings_path.display());
        return Ok(());
    }

    let raw = std::fs::read_to_string(&settings_path)
        .with_context(|| format!("Đọc {}", settings_path.display()))?;
    let mut root: Value = serde_json::from_str(&raw)
        .with_context(|| "Parse settings.json")?;

    let hooks_val = root.get("hooks").cloned().unwrap_or_else(|| json!({}));
    if !has_c2_tracker_entry(&hooks_val) {
        println!("Không tìm thấy entry C2-Tracker nào trong {}", settings_path.display());
        return Ok(());
    }

    backup_settings(&settings_path)?;

    if let Some(hooks_obj) = root.get_mut("hooks") {
        remove_c2_tracker_entries(hooks_obj);
    }

    let out = serde_json::to_string_pretty(&root)?;
    std::fs::write(&settings_path, out)
        .with_context(|| format!("Ghi {}", settings_path.display()))?;

    println!("Da gỡ hooks C2-Tracker khỏi {}", settings_path.display());
    Ok(())
}
