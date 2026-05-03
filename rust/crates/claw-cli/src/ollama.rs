use std::io::{BufRead, BufReader, Write};
use std::time::{Duration, Instant};

use serde::Deserialize;

const DEFAULT_BASE_URL: &str = "http://localhost:11434";
const PULL_REQUEST_TIMEOUT: Duration = Duration::from_secs(60 * 60);

fn base_url() -> String {
    std::env::var("OLLAMA_BASE_URL")
        .ok()
        .map(|raw| raw.trim_end_matches("/v1").trim_end_matches('/').to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_BASE_URL.to_string())
}

fn client() -> reqwest::blocking::Client {
    reqwest::blocking::Client::builder()
        .timeout(None)
        .build()
        .expect("blocking client")
}

#[derive(Debug, Deserialize)]
struct TagsResponse {
    #[serde(default)]
    models: Vec<TagModel>,
}

#[derive(Debug, Deserialize)]
struct TagModel {
    name: String,
    #[serde(default)]
    size: u64,
}

#[derive(Debug, Clone)]
pub struct InstalledModel {
    pub name: String,
    pub size_bytes: u64,
}

pub fn daemon_reachable() -> bool {
    use std::net::TcpStream;
    let url = base_url();
    let host_port = url
        .trim_start_matches("http://")
        .trim_start_matches("https://");
    TcpStream::connect_timeout(
        &host_port
            .parse()
            .unwrap_or_else(|_| "127.0.0.1:11434".parse().expect("fallback addr")),
        Duration::from_millis(200),
    )
    .is_ok()
}

pub fn list_pulled() -> Result<Vec<InstalledModel>, String> {
    if !daemon_reachable() {
        return Ok(Vec::new());
    }
    let response = client()
        .get(format!("{}/api/tags", base_url()))
        .timeout(Duration::from_secs(2))
        .send()
        .map_err(|e| format!("ollama tags request failed: {e}"))?;
    if !response.status().is_success() {
        return Err(format!("ollama /api/tags returned {}", response.status()));
    }
    let payload: TagsResponse = response
        .json()
        .map_err(|e| format!("ollama tags parse failed: {e}"))?;
    Ok(payload
        .models
        .into_iter()
        .map(|m| InstalledModel {
            name: m.name,
            size_bytes: m.size,
        })
        .collect())
}

pub fn is_installed(name: &str) -> bool {
    list_pulled()
        .map(|installed| installed.iter().any(|m| m.name == name))
        .unwrap_or(false)
}

#[derive(Debug, Deserialize)]
struct PullEvent {
    #[serde(default)]
    status: String,
    #[serde(default)]
    digest: Option<String>,
    #[serde(default)]
    total: Option<u64>,
    #[serde(default)]
    completed: Option<u64>,
    #[serde(default)]
    error: Option<String>,
}

pub fn pull_with_progress(name: &str, mut writer: impl Write) -> Result<(), String> {
    if !daemon_reachable() {
        return Err("ollama daemon not reachable on localhost:11434".to_string());
    }
    let body = serde_json::json!({ "name": name, "stream": true });
    let response = client()
        .post(format!("{}/api/pull", base_url()))
        .timeout(PULL_REQUEST_TIMEOUT)
        .json(&body)
        .send()
        .map_err(|e| format!("ollama pull request failed: {e}"))?;
    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().unwrap_or_default();
        return Err(format!("ollama /api/pull {status}: {body}"));
    }

    let started = Instant::now();
    let mut last_render = Instant::now() - Duration::from_secs(1);
    let mut last_digest: Option<String> = None;
    let mut last_completed: u64 = 0;
    let reader = BufReader::new(response);
    for line in reader.lines() {
        let line = line.map_err(|e| format!("ollama pull stream error: {e}"))?;
        if line.trim().is_empty() {
            continue;
        }
        let event: PullEvent = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => continue,
        };
        if let Some(error) = event.error {
            clear_line(&mut writer);
            return Err(format!("ollama pull error: {error}"));
        }
        match (event.total, event.completed) {
            (Some(total), Some(completed)) if total > 0 => {
                if last_digest.as_deref() != event.digest.as_deref() {
                    if last_digest.is_some() {
                        clear_line(&mut writer);
                        let _ = writeln!(writer);
                    }
                    last_digest = event.digest.clone();
                }
                if last_render.elapsed() >= Duration::from_millis(150)
                    || completed == total
                {
                    let elapsed = started.elapsed().as_secs_f64().max(0.001);
                    let speed = completed as f64 / elapsed;
                    render_progress(
                        &mut writer,
                        &event.status,
                        completed,
                        total,
                        speed,
                    );
                    last_render = Instant::now();
                    last_completed = completed;
                }
            }
            _ => {
                clear_line(&mut writer);
                let _ = write!(writer, "  {}", event.status);
                let _ = writer.flush();
            }
        }
        if event.status == "success" {
            clear_line(&mut writer);
            let _ = writeln!(writer, "  pull complete · {}", format_bytes(last_completed));
            return Ok(());
        }
    }
    clear_line(&mut writer);
    let _ = writeln!(writer, "  pull stream ended");
    Ok(())
}

fn render_progress(
    writer: &mut impl Write,
    status: &str,
    completed: u64,
    total: u64,
    speed_bps: f64,
) {
    let pct = (completed as f64 / total as f64 * 100.0).min(100.0);
    let bar_width = 20usize;
    let filled = ((pct / 100.0) * bar_width as f64).round() as usize;
    let bar: String = (0..bar_width)
        .map(|i| if i < filled { '#' } else { '.' })
        .collect();
    let eta_secs = if speed_bps > 0.0 {
        ((total - completed) as f64 / speed_bps) as u64
    } else {
        0
    };
    let _ = write!(
        writer,
        "\r  {short_status:<22} [{bar}] {pct:5.1}% · {done} / {total_h} · {speed}/s · ETA {eta}",
        short_status = truncate(status, 22),
        bar = bar,
        pct = pct,
        done = format_bytes(completed),
        total_h = format_bytes(total),
        speed = format_bytes(speed_bps as u64),
        eta = format_duration(eta_secs),
    );
    let _ = writer.flush();
}

fn clear_line(writer: &mut impl Write) {
    let _ = write!(writer, "\r\x1b[2K");
    let _ = writer.flush();
}

fn truncate(text: &str, width: usize) -> String {
    if text.chars().count() <= width {
        text.to_string()
    } else {
        text.chars().take(width.saturating_sub(1)).collect::<String>() + "…"
    }
}

pub fn format_bytes(n: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut value = n as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} B", n)
    } else if value >= 100.0 {
        format!("{:.0} {}", value, UNITS[unit])
    } else {
        format!("{:.1} {}", value, UNITS[unit])
    }
}

fn format_duration(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}h{:02}m", secs / 3600, (secs % 3600) / 60)
    } else if secs >= 60 {
        format!("{}m{:02}s", secs / 60, secs % 60)
    } else {
        format!("{}s", secs)
    }
}

