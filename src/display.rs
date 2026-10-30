//! 终端输出：颜色、计数格式化、表格。

use std::io::IsTerminal;
use std::sync::OnceLock;

use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, CellAlignment, ContentArrangement, Table};
use serde_json::Value;
use unicode_width::UnicodeWidthStr;

fn color_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED
        .get_or_init(|| std::env::var_os("NO_COLOR").is_none() && std::io::stdout().is_terminal())
}

fn paint(code: &str, text: &str) -> String {
    if color_enabled() {
        format!("\x1b[{}m{}\x1b[0m", code, text)
    } else {
        text.to_string()
    }
}

pub fn bold(text: &str) -> String {
    paint("1", text)
}

pub fn dim(text: &str) -> String {
    paint("2", text)
}

pub fn title(text: &str) -> String {
    paint("1;36", text)
}

pub fn success(msg: &str) {
    println!("  {} {}", paint("1;32", "✓"), msg);
}

pub fn error(msg: &str) {
    println!("  {} {}", paint("1;31", "✗"), msg);
}

pub fn warning(msg: &str) {
    println!("  {} {}", paint("1;33", "!"), msg);
}

pub fn info(msg: &str) {
    println!("  {} {}", paint("36", "›"), msg);
}

pub fn hint(msg: &str) {
    println!("  {}", dim(&format!("hint: {}", msg)));
}

/// 终端可用宽度（留一点边距）。
pub fn term_width() -> usize {
    let from_env = std::env::var("COLUMNS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok());
    let width = from_env
        .or_else(|| terminal_size::terminal_size().map(|(w, _)| w.0 as usize))
        .unwrap_or(100);
    width.saturating_sub(2).max(40)
}

/// 正文渲染宽度：终端特别宽时限制在 100 列，避免一行太长。
pub fn content_width() -> usize {
    term_width().min(100)
}

pub fn fmt_count(n: i64) -> String {
    if n >= 100_000_000 {
        format!("{:.1}亿", n as f64 / 100_000_000.0)
    } else if n >= 10_000 {
        format!("{:.1}万", n as f64 / 10_000.0)
    } else {
        n.to_string()
    }
}

pub fn format_count(value: Option<&Value>) -> String {
    match value {
        Some(Value::Number(n)) => fmt_count(n.as_i64().unwrap_or(0)),
        Some(Value::String(s)) => s
            .parse::<i64>()
            .map(fmt_count)
            .unwrap_or_else(|_| s.clone()),
        _ => "0".to_string(),
    }
}

pub fn truncate(text: &str, max_width: usize) -> String {
    let text = text.replace('\n', " ");
    let text = text.trim();
    if text.width() <= max_width {
        return text.to_string();
    }
    let mut out = String::new();
    let mut width = 0;
    for ch in text.chars() {
        let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        if width + w + 1 > max_width {
            break;
        }
        out.push(ch);
        width += w;
    }
    out.push('…');
    out
}

pub fn stats_line(pairs: &[(&str, String)]) -> String {
    pairs
        .iter()
        .map(|(label, value)| format!("{} {} {}", dim("▸"), bold(value), dim(label)))
        .collect::<Vec<_>>()
        .join("  ")
}

/// 打印一个带标题的表格。`right_align` 是需要右对齐的列索引。
pub fn print_table(
    table_title: Option<&str>,
    headers: &[&str],
    rows: Vec<Vec<String>>,
    right_align: &[usize],
) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_width(term_width().min(160) as u16);

    table.set_header(
        headers
            .iter()
            .map(|h| Cell::new(*h).set_alignment(CellAlignment::Center))
            .collect::<Vec<_>>(),
    );

    for row in rows {
        table.add_row(
            row.into_iter()
                .enumerate()
                .map(|(i, cell)| {
                    let cell = Cell::new(cell);
                    if right_align.contains(&i) {
                        cell.set_alignment(CellAlignment::Right)
                    } else {
                        cell
                    }
                })
                .collect::<Vec<_>>(),
        );
    }

    if let Some(t) = table_title {
        println!();
        println!("  {}", title(t));
    } else {
        println!();
    }
    println!("{table}");
    println!();
}
