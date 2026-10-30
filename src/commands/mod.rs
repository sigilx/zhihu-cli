pub mod auth;
pub mod content;
pub mod interact;
pub mod user;

use anyhow::{anyhow, Result};
use serde_json::Value;

use crate::auth::{cookie_str_to_dict, get_cookie_string};
use crate::client::ZhihuClient;

pub fn client() -> Result<ZhihuClient> {
    let cookie =
        get_cookie_string().ok_or_else(|| anyhow!("未登录 — 请先运行 zhihu login --qrcode"))?;
    ZhihuClient::new(&cookie_str_to_dict(&cookie))
}

pub fn print_json(value: &Value) {
    match serde_json::to_string_pretty(value) {
        Ok(s) => println!("{s}"),
        Err(_) => println!("{value}"),
    }
}

pub fn value_to_id(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Number(n)) => n.to_string(),
        _ => String::new(),
    }
}

/// 打印 key-value 资料卡，值为长文本时自动换行并对齐。
pub fn print_kv(heading: &str, pairs: &[(String, String)]) {
    let key_width = 12usize;
    let padding = 2 + key_width + 2;
    let width = crate::display::content_width()
        .saturating_sub(padding)
        .max(12);
    println!();
    println!("  {}", crate::display::title(heading));
    for (key, value) in pairs {
        if value.trim().is_empty() {
            continue;
        }
        let text = value.replace('\n', " ");
        let lines = crate::html::wrap_line(text.trim(), width);
        for (i, line) in lines.iter().enumerate() {
            if i == 0 {
                println!(
                    "  {}  {}",
                    crate::display::dim(&format!("{:<width$}", key, width = key_width)),
                    line
                );
            } else {
                println!("{}{}", " ".repeat(padding), line);
            }
        }
    }
    println!();
}
