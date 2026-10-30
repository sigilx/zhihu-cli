//! 认证命令：login / logout / status / whoami。

use anyhow::{bail, Result};

use crate::auth::{
    clear_cookies, cookie_str_to_dict, get_cookie_string, get_saved_cookie_string, qrcode_login,
    save_cookies,
};
use crate::client::ZhihuClient;
use crate::commands::{client, print_json, print_kv};
use crate::config::REQUIRED_COOKIES;
use crate::display;
use crate::html::html_to_inline;
use crate::util::json_i64;

fn verify_cookies(cookies: &std::collections::HashMap<String, String>) -> Option<bool> {
    let client = ZhihuClient::new(cookies).ok()?;
    let info = client.get_self_info().ok()?;
    if !info.is_object() || info.as_object().is_some_and(|o| o.is_empty()) {
        return None;
    }
    let name = info.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let id = info.get("id").map(|v| !v.is_null()).unwrap_or(false);
    if (!name.is_empty() && name != "知乎用户") || id {
        Some(true)
    } else {
        Some(false)
    }
}

pub fn login(qrcode: bool, cookie_str: Option<String>) -> Result<()> {
    if let Some(raw) = cookie_str {
        let parsed = cookie_str_to_dict(&raw);
        if !REQUIRED_COOKIES.iter().all(|k| parsed.contains_key(*k)) {
            display::error("Cookie 无效 — 必须包含 z_c0、_xsrf、d_c0");
            std::process::exit(1);
        }
        save_cookies(&parsed)?;
        display::success("Cookie 已保存");
        return Ok(());
    }

    if !qrcode {
        if let Some(cookie) = get_cookie_string() {
            match verify_cookies(&cookie_str_to_dict(&cookie)) {
                Some(true) => {
                    display::success("已登录（本地 cookie）");
                    return Ok(());
                }
                Some(false) => {
                    display::warning("本地 cookie 已失效，开始重新登录");
                    clear_cookies()?;
                }
                None => {
                    display::warning("无法验证 cookie — 保留现有登录态");
                    return Ok(());
                }
            }
        }
    }

    display::info("启动二维码登录…");
    let cookies = qrcode_login()?;
    match verify_cookies(&cookies) {
        Some(false) => {
            clear_cookies()?;
            bail!("登录完成但会话无效，请重试");
        }
        _ => display::success("登录成功 — cookie 已保存"),
    }
    Ok(())
}

pub fn logout() -> Result<()> {
    let removed = clear_cookies()?;
    if removed.is_empty() {
        display::warning("没有保存的登录态");
    } else {
        display::success(&format!("已退出登录 — 删除: {}", removed.join(", ")));
    }
    Ok(())
}

pub fn status() -> Result<()> {
    if get_saved_cookie_string().is_none() {
        display::error("未登录");
        display::hint("运行 zhihu login --qrcode 或 zhihu login --cookie \"...\"");
        std::process::exit(1);
    }
    display::success("已登录（本地保存的 cookie）");
    display::hint("运行 zhihu whoami 查看个人资料");
    Ok(())
}

pub fn whoami(as_json: bool) -> Result<()> {
    let client = client()?;
    let info = client.get_self_info()?;
    if as_json {
        print_json(&info);
        return Ok(());
    }

    let name = {
        let n = info.get("name").and_then(|v| v.as_str()).unwrap_or("");
        if n.is_empty() {
            "未知用户".to_string()
        } else {
            n.to_string()
        }
    };
    let url_token = info.get("url_token").and_then(|v| v.as_str()).unwrap_or("");
    let gender = match json_i64(&info, "gender") {
        0 => "男",
        1 => "女",
        _ => "—",
    };
    let pairs = vec![
        ("ID".to_string(), format!("@{}", url_token)),
        (
            "Headline".to_string(),
            info.get("headline")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
        ),
        (
            "Bio".to_string(),
            html_to_inline(
                info.get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
            ),
        ),
        ("Gender".to_string(), gender.to_string()),
        (
            "Answers".to_string(),
            display::format_count(info.get("answer_count")),
        ),
        (
            "Articles".to_string(),
            display::format_count(info.get("articles_count")),
        ),
        (
            "Followers".to_string(),
            display::format_count(info.get("follower_count")),
        ),
        (
            "Following".to_string(),
            display::format_count(info.get("following_count")),
        ),
        (
            "Upvotes".to_string(),
            display::format_count(info.get("voteup_count")),
        ),
        (
            "Thanks".to_string(),
            display::format_count(info.get("thanked_count")),
        ),
    ];
    print_kv(&name, &pairs);
    Ok(())
}
