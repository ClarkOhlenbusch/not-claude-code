use std::io::{self, IsTerminal, Write};

use crossterm::cursor::{Hide, MoveToColumn, MoveUp, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::{execute, queue};

use crate::ollama;

#[derive(Debug, Clone)]
pub struct PickerEntry {
    pub alias: &'static str,
    pub canonical: &'static str,
    pub kind: EntryKind,
    pub estimated_gb: Option<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    Cloud,
    Ollama,
    /// Multi-model orchestrator (intent → planner → coder → reviewer + retry).
    /// Not a single model — selects the OrchestratorRuntime path. Routes
    /// individual role calls through whatever Ollama models are configured
    /// in `RoleConfig`.
    Swarm,
}

const STATIC_ENTRIES: &[PickerEntry] = &[
    // Default — the orchestrator (intent → planner → coder → reviewer).
    // Listed first so it appears at the top of the picker.
    PickerEntry {
        alias: "swarm",
        canonical: "swarm",
        kind: EntryKind::Swarm,
        estimated_gb: None,
    },
    PickerEntry {
        alias: "opus",
        canonical: "claude-opus-4-6",
        kind: EntryKind::Cloud,
        estimated_gb: None,
    },
    PickerEntry {
        alias: "sonnet",
        canonical: "claude-sonnet-4-6",
        kind: EntryKind::Cloud,
        estimated_gb: None,
    },
    PickerEntry {
        alias: "haiku",
        canonical: "claude-haiku-4-5-20251213",
        kind: EntryKind::Cloud,
        estimated_gb: None,
    },
    PickerEntry {
        alias: "grok",
        canonical: "grok-3",
        kind: EntryKind::Cloud,
        estimated_gb: None,
    },
    PickerEntry {
        alias: "grok-mini",
        canonical: "grok-3-mini",
        kind: EntryKind::Cloud,
        estimated_gb: None,
    },
    PickerEntry {
        alias: "qwen-coder",
        canonical: "qwen3-coder:30b",
        kind: EntryKind::Ollama,
        estimated_gb: Some(17.0),
    },
    PickerEntry {
        alias: "glm-flash",
        canonical: "glm-4.7-flash:q4",
        kind: EntryKind::Ollama,
        estimated_gb: Some(17.0),
    },
    PickerEntry {
        alias: "gemma",
        canonical: "google/gemma-4-31B-it",
        kind: EntryKind::Cloud,
        estimated_gb: None,
    },
    PickerEntry {
        alias: "runpod-gemma4-31b",
        canonical: "google/gemma-4-31B-it",
        kind: EntryKind::Cloud,
        estimated_gb: None,
    },
];

#[derive(Debug, Clone)]
pub struct ResolvedEntry {
    pub label: String,
    pub canonical: String,
    pub kind: EntryKind,
    pub status: EntryStatus,
}

#[derive(Debug, Clone)]
pub enum EntryStatus {
    Cloud,
    Ready {
        size_bytes: u64,
    },
    NotPulled {
        estimated_gb: Option<f32>,
    },
    /// Always available — the swarm path doesn't pull a model itself,
    /// it dispatches to whatever the configured roles use.
    Swarm,
}

pub fn build_entries(installed: &[ollama::InstalledModel]) -> Vec<ResolvedEntry> {
    let mut entries = Vec::new();
    for entry in STATIC_ENTRIES {
        let status = match entry.kind {
            EntryKind::Cloud => EntryStatus::Cloud,
            EntryKind::Swarm => EntryStatus::Swarm,
            EntryKind::Ollama => installed
                .iter()
                .find(|m| m.name == entry.canonical)
                .map(|m| EntryStatus::Ready {
                    size_bytes: m.size_bytes,
                })
                .unwrap_or(EntryStatus::NotPulled {
                    estimated_gb: entry.estimated_gb,
                }),
        };
        let label = if entry.alias == entry.canonical {
            entry.alias.to_string()
        } else {
            format!("{}  →  {}", entry.alias, entry.canonical)
        };
        entries.push(ResolvedEntry {
            label,
            canonical: entry.canonical.to_string(),
            kind: entry.kind,
            status,
        });
    }

    let known: std::collections::HashSet<&str> =
        STATIC_ENTRIES.iter().map(|e| e.canonical).collect();
    for installed_model in installed {
        if known.contains(installed_model.name.as_str()) {
            continue;
        }
        entries.push(ResolvedEntry {
            label: installed_model.name.clone(),
            canonical: installed_model.name.clone(),
            kind: EntryKind::Ollama,
            status: EntryStatus::Ready {
                size_bytes: installed_model.size_bytes,
            },
        });
    }
    entries
}

#[derive(Debug)]
enum Row {
    Header(&'static str),
    Item(usize),
    Spacer,
}

fn build_rows(entries: &[ResolvedEntry]) -> Vec<Row> {
    let mut rows = Vec::new();
    let cloud_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.kind, EntryKind::Cloud))
        .map(|(i, _)| i)
        .collect();
    let local_indices: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.kind, EntryKind::Ollama))
        .map(|(i, _)| i)
        .collect();
    if !cloud_indices.is_empty() {
        rows.push(Row::Header("Cloud"));
        for i in cloud_indices {
            rows.push(Row::Item(i));
        }
    }
    if !local_indices.is_empty() {
        if !rows.is_empty() {
            rows.push(Row::Spacer);
        }
        rows.push(Row::Header("Local · Ollama"));
        for i in local_indices {
            rows.push(Row::Item(i));
        }
    }
    rows
}

pub enum PickerOutcome {
    Selected(String),
    Cancelled,
}

pub fn run(current_model: &str) -> io::Result<PickerOutcome> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Ok(PickerOutcome::Cancelled);
    }

    let installed = ollama::list_pulled().unwrap_or_default();
    let entries = build_entries(&installed);
    let rows = build_rows(&entries);

    let initial_cursor = rows
        .iter()
        .position(|row| matches!(row, Row::Item(i) if entries[*i].canonical == current_model))
        .unwrap_or_else(|| {
            rows.iter()
                .position(|r| matches!(r, Row::Item(_)))
                .unwrap_or(0)
        });

    terminal::enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, Hide)?;
    let result = run_loop(&mut stdout, &entries, &rows, initial_cursor, current_model);
    let _ = clear_render(&mut stdout, &rows);
    let _ = execute!(stdout, Show);
    let _ = terminal::disable_raw_mode();
    result
}

fn run_loop(
    stdout: &mut io::Stdout,
    entries: &[ResolvedEntry],
    rows: &[Row],
    mut cursor: usize,
    current_model: &str,
) -> io::Result<PickerOutcome> {
    render(stdout, entries, rows, cursor, current_model)?;
    loop {
        let Event::Key(key) = event::read()? else {
            continue;
        };
        if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            return Ok(PickerOutcome::Cancelled);
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => {
                cursor = move_cursor(rows, cursor, -1);
            }
            KeyCode::Down | KeyCode::Char('j') => {
                cursor = move_cursor(rows, cursor, 1);
            }
            KeyCode::Enter => {
                if let Row::Item(idx) = rows[cursor] {
                    return Ok(PickerOutcome::Selected(entries[idx].canonical.clone()));
                }
            }
            KeyCode::Esc | KeyCode::Char('q') => {
                return Ok(PickerOutcome::Cancelled);
            }
            _ => {}
        }
        render(stdout, entries, rows, cursor, current_model)?;
    }
}

fn move_cursor(rows: &[Row], cursor: usize, delta: isize) -> usize {
    let len = rows.len() as isize;
    let mut idx = cursor as isize;
    for _ in 0..len {
        idx = (idx + delta).rem_euclid(len);
        if matches!(rows[idx as usize], Row::Item(_)) {
            return idx as usize;
        }
    }
    cursor
}

fn render(
    stdout: &mut io::Stdout,
    entries: &[ResolvedEntry],
    rows: &[Row],
    cursor: usize,
    current_model: &str,
) -> io::Result<()> {
    clear_render(stdout, rows)?;
    queue!(
        stdout,
        SetForegroundColor(Color::DarkGrey),
        Print("Select model · ↑↓ enter ↵ cancel esc"),
        ResetColor,
    )?;
    queue!(stdout, Print("\r\n"))?;

    for (row_index, row) in rows.iter().enumerate() {
        match row {
            Row::Header(title) => {
                queue!(
                    stdout,
                    SetForegroundColor(Color::DarkGrey),
                    Print(format!("  {title}")),
                    ResetColor,
                    Print("\r\n"),
                )?;
            }
            Row::Spacer => {
                queue!(stdout, Print("\r\n"))?;
            }
            Row::Item(idx) => {
                let entry = &entries[*idx];
                let is_selected = row_index == cursor;
                let is_current = entry.canonical == current_model;
                let prefix = if is_selected { ">" } else { " " };
                let marker = if is_current { "*" } else { " " };
                let status = render_status(&entry.status);
                let status_color = match &entry.status {
                    EntryStatus::Ready { .. } => Color::Green,
                    EntryStatus::NotPulled { .. } => Color::DarkYellow,
                    EntryStatus::Cloud => Color::Cyan,
                    EntryStatus::Swarm => Color::Magenta,
                };
                if is_selected {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Yellow),
                        Print(format!("{prefix} {marker} ")),
                        ResetColor,
                    )?;
                } else {
                    queue!(stdout, Print(format!("{prefix} {marker} ")))?;
                }
                let label_width = 38;
                let label_padded = pad_right(&entry.label, label_width);
                if is_selected {
                    queue!(
                        stdout,
                        SetForegroundColor(Color::Yellow),
                        Print(label_padded),
                        ResetColor,
                    )?;
                } else {
                    queue!(stdout, Print(label_padded))?;
                }
                queue!(
                    stdout,
                    SetForegroundColor(status_color),
                    Print(format!("  {status}")),
                    ResetColor,
                    Print("\r\n"),
                )?;
            }
        }
    }
    stdout.flush()?;
    Ok(())
}

fn clear_render(stdout: &mut io::Stdout, rows: &[Row]) -> io::Result<()> {
    let total_lines = 1 + rows.len();
    queue!(stdout, MoveToColumn(0))?;
    for _ in 0..total_lines {
        queue!(stdout, Clear(ClearType::CurrentLine), MoveUp(1))?;
    }
    queue!(stdout, MoveToColumn(0), Clear(ClearType::FromCursorDown))?;
    stdout.flush()?;
    Ok(())
}

fn render_status(status: &EntryStatus) -> String {
    match status {
        EntryStatus::Cloud => "cloud".to_string(),
        EntryStatus::Swarm => "orchestrator · plan→code→review".to_string(),
        EntryStatus::Ready { size_bytes } => {
            format!("ready · {}", ollama::format_bytes(*size_bytes))
        }
        EntryStatus::NotPulled { estimated_gb } => match estimated_gb {
            Some(gb) => format!("pull on switch · ~{gb:.0} GB"),
            None => "pull on switch".to_string(),
        },
    }
}

fn pad_right(text: &str, width: usize) -> String {
    let len = text.chars().count();
    if len >= width {
        text.to_string()
    } else {
        let pad = " ".repeat(width - len);
        format!("{text}{pad}")
    }
}
