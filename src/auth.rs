//! 登录态管理：cookie 文件、二维码登录。

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use image::{GrayImage, Luma};
use qrcode::{Color as QrColor, QrCode};
use serde_json::{json, Value};

use crate::client::{add_cookies, build_http, jar_cookies, new_jar};
use crate::config::{
    config_dir, cookie_file, qrcode_image_path, REQUIRED_COOKIES, ZHIHU_BASE_URL, ZHIHU_LOGIN_URL,
    ZHIHU_OAUTH_CAPTCHA, ZHIHU_QRCODE_API,
};
use crate::display;

pub fn cookie_str_to_dict(cookie_str: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for item in cookie_str.split(';') {
        let item = item.trim();
        if let Some((k, v)) = item.split_once('=') {
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
    }
    map
}

pub fn dict_to_cookie_str(cookies: &HashMap<String, String>) -> String {
    cookies
        .iter()
        .map(|(k, v)| format!("{}={}", k, v))
        .collect::<Vec<_>>()
        .join("; ")
}

pub fn has_required_cookies(cookies: &HashMap<String, String>) -> bool {
    REQUIRED_COOKIES.iter().all(|k| cookies.contains_key(*k))
}

pub fn save_cookies(cookies: &HashMap<String, String>) -> Result<()> {
    let dir = config_dir();
    std::fs::create_dir_all(&dir).context("创建配置目录失败")?;
    let file = cookie_file();
    let data = json!({ "cookies": cookies });
    std::fs::write(&file, serde_json::to_string_pretty(&data)?)
        .with_context(|| format!("写入 {} 失败", file.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn clear_cookies() -> Result<Vec<String>> {
    let file = cookie_file();
    let mut removed = Vec::new();
    if file.exists() {
        std::fs::remove_file(&file).with_context(|| format!("删除 {} 失败", file.display()))?;
        removed.push(
            file.file_name()
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_default(),
        );
    }
    Ok(removed)
}

/// 只读取本地保存的 cookie，不发起网络请求。
pub fn get_saved_cookie_string() -> Option<String> {
    let file = cookie_file();
    if !file.exists() {
        return None;
    }
    let text = std::fs::read_to_string(&file).ok()?;
    let data: Value = serde_json::from_str(&text).ok()?;
    let obj = data.get("cookies")?.as_object()?;
    let mut cookies = HashMap::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            cookies.insert(k.clone(), s.to_string());
        }
    }
    if has_required_cookies(&cookies) {
        Some(dict_to_cookie_str(&cookies))
    } else {
        None
    }
}

/// 读取 cookie；如果只有 z_c0 而缺少 _xsrf / d_c0，会访问知乎补全并保存。
pub fn get_cookie_string() -> Option<String> {
    let file = cookie_file();
    if !file.exists() {
        return None;
    }
    let text = std::fs::read_to_string(&file).ok()?;
    let data: Value = serde_json::from_str(&text).ok()?;
    let obj = data.get("cookies")?.as_object()?;
    let mut cookies = HashMap::new();
    for (k, v) in obj {
        if let Some(s) = v.as_str() {
            cookies.insert(k.clone(), s.to_string());
        }
    }
    if has_required_cookies(&cookies) {
        return Some(dict_to_cookie_str(&cookies));
    }
    if !cookies.contains_key("z_c0") {
        return None;
    }
    let merged = fetch_missing_cookies(&cookies).unwrap_or(cookies);
    if has_required_cookies(&merged) {
        let _ = save_cookies(&merged);
        return Some(dict_to_cookie_str(&merged));
    }
    None
}

fn fetch_missing_cookies(cookies: &HashMap<String, String>) -> Option<HashMap<String, String>> {
    if !cookies.contains_key("z_c0") {
        return None;
    }
    let jar = new_jar();
    add_cookies(&jar, cookies);
    let http = build_http(jar.clone()).ok()?;
    let _ = http.get(format!("{}/", ZHIHU_BASE_URL)).send().ok()?;
    let mut merged = cookies.clone();
    for name in ["_xsrf", "d_c0"] {
        if let Some(v) = jar_cookies(&jar).get(name) {
            merged.insert(name.to_string(), v.clone());
        }
    }
    Some(merged)
}

// ── 二维码登录 ────────────────────────────────────────────────────────────────

pub fn qrcode_login() -> Result<HashMap<String, String>> {
    let jar = new_jar();
    let http = build_http(jar.clone())?;

    // 1. 登录页，拿到 _xsrf 等初始 cookie
    let _ = http
        .get(ZHIHU_LOGIN_URL)
        .header("Referer", format!("{}/", ZHIHU_BASE_URL))
        .header("Origin", ZHIHU_BASE_URL)
        .send();

    // 2. udid（生成 d_c0 / q_c1）
    let _ = http
        .post(format!("{}/udid", ZHIHU_BASE_URL))
        .json(&json!({}))
        .send();

    // 3. capsion_ticket（扫码确认流程需要）
    let _ = http.get(ZHIHU_OAUTH_CAPTCHA).send();

    // 4. 获取二维码 token 与链接
    let xsrf = jar_cookies(&jar).get("_xsrf").cloned();
    let mut req = http
        .post(ZHIHU_QRCODE_API)
        .header("Referer", format!("{}/signin", ZHIHU_BASE_URL))
        .header("Origin", ZHIHU_BASE_URL)
        .header("x-requested-with", "fetch")
        .json(&json!({}));
    if let Some(x) = &xsrf {
        req = req.header("x-xsrftoken", x);
    }
    let resp = req.send().context("获取二维码失败")?;
    if !resp.status().is_success() {
        bail!("获取二维码失败: HTTP {}", resp.status());
    }
    let data: Value = resp.json().context("二维码接口返回无效 JSON")?;
    let token = data
        .get("token")
        .or_else(|| data.get("qrcode_token"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let link = data.get("link").and_then(Value::as_str).unwrap_or("");
    if token.is_empty() || link.is_empty() {
        bail!("二维码接口未返回 token 或 link");
    }

    // 5. 保存 PNG，并在终端显示
    if let Err(e) = save_qrcode_png(link) {
        display::hint(&format!("二维码图片保存失败: {e}"));
    }
    display::info("请使用知乎 App 扫描下方二维码登录");
    println!("\n{}", render_qr_terminal(link)?);
    display::hint(&format!("若终端无法显示二维码，请用手机浏览器打开: {link}"));
    println!();
    display::info("请在手机上点击「确认登录」…");

    // 6. 轮询扫码状态
    let scan_url = format!("{}/{}/scan_info", ZHIHU_QRCODE_API, token);
    let deadline = Instant::now() + Duration::from_secs(120);
    let mut success = false;
    while Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(150));
        let xsrf = jar_cookies(&jar).get("_xsrf").cloned();
        let mut req = http
            .get(&scan_url)
            .header("Referer", format!("{}/signin?next=%2F", ZHIHU_BASE_URL))
            .header("Accept", "*/*")
            .header("sec-fetch-dest", "empty")
            .header("sec-fetch-mode", "cors")
            .header("sec-fetch-site", "same-origin")
            .header("x-requested-with", "fetch")
            .header("x-zse-93", "101_3_3.0");
        if let Some(x) = &xsrf {
            req = req.header("x-xsrftoken", x);
        }
        let resp = match req.send() {
            Ok(r) => r,
            Err(_) => continue,
        };
        let now_cookies = jar_cookies(&jar);
        if now_cookies.contains_key("z_c0") {
            success = true;
            break;
        }
        if !resp.status().is_success() {
            continue;
        }
        let info: Value = resp.json().unwrap_or_else(|_| json!({}));
        let api_status = info.get("status").and_then(Value::as_i64);
        if api_status == Some(0) || api_status == Some(1) {
            continue;
        }
        if info
            .get("access_token")
            .map(|v| !v.is_null())
            .unwrap_or(false)
            || info.get("user_id").map(|v| !v.is_null()).unwrap_or(false)
        {
            success = true;
            break;
        }
        let login_status = info
            .get("login_status")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_uppercase();
        if matches!(
            login_status.as_str(),
            "CONFIRMED" | "LOGIN_SUCCESS" | "SUCCESS" | "OK" | "LOGGED_IN"
        ) {
            success = true;
            break;
        }
        if info.get("success").and_then(Value::as_bool) == Some(true)
            || info.get("logged_in").and_then(Value::as_bool) == Some(true)
        {
            success = true;
            break;
        }
        // 接口有时把 cookie 直接放在 body 里
        if let Some(cookie_str) = info
            .get("cookie")
            .or_else(|| info.get("cookies"))
            .and_then(Value::as_str)
        {
            if cookie_str.contains("z_c0") {
                add_cookies(&jar, &cookie_str_to_dict(cookie_str));
            }
        }
        if let Some(z) = info.get("z_c0").and_then(Value::as_str) {
            let mut m = HashMap::new();
            m.insert("z_c0".to_string(), z.to_string());
            add_cookies(&jar, &m);
        }
        if jar_cookies(&jar).contains_key("z_c0") {
            success = true;
            break;
        }
    }

    if !success {
        // 再请求一次需要登录的接口，触发完整 cookie
        let _ = http.get(format!("{}/api/v4/me", ZHIHU_BASE_URL)).send();
    }

    let cookies = jar_cookies(&jar);
    if !has_required_cookies(&cookies) {
        bail!("二维码登录超时或未完成确认（未获取到 z_c0）");
    }
    save_cookies(&cookies)?;
    Ok(cookies)
}

fn save_qrcode_png(link: &str) -> Result<()> {
    let code = QrCode::new(link.as_bytes()).context("生成二维码失败")?;
    let width = code.width();
    let colors = code.to_colors();
    let scale = 8u32;
    let border = 4u32;
    let size = (width as u32 + border * 2) * scale;
    let mut img = GrayImage::from_pixel(size, size, Luma([255u8]));
    for y in 0..width {
        for x in 0..width {
            if colors[y * width + x] == QrColor::Dark {
                let px = (x as u32 + border) * scale;
                let py = (y as u32 + border) * scale;
                for dy in 0..scale {
                    for dx in 0..scale {
                        img.put_pixel(px + dx, py + dy, Luma([0u8]));
                    }
                }
            }
        }
    }
    let dir = config_dir();
    std::fs::create_dir_all(&dir).ok();
    let path = qrcode_image_path();
    img.save(&path)
        .with_context(|| format!("保存二维码到 {} 失败", path.display()))?;
    display::hint(&format!("二维码已保存至: {}", path.display()));
    Ok(())
}

fn render_qr_terminal(link: &str) -> Result<String> {
    let code = QrCode::new(link.as_bytes()).context("生成二维码失败")?;
    let width = code.width();
    let colors = code.to_colors();
    let border = 2usize;
    let padded = width + border * 2;
    let is_dark = |x: usize, y: usize| -> bool {
        if x < border || y < border || x >= border + width || y >= border + width {
            false
        } else {
            colors[(y - border) * width + (x - border)] == QrColor::Dark
        }
    };
    let mut out = String::new();
    for y in (0..padded).step_by(2) {
        out.push_str("  ");
        for x in 0..padded {
            let top = is_dark(x, y);
            let bottom = is_dark(x, y + 1);
            out.push(match (top, bottom) {
                (false, false) => ' ',
                (true, false) => '▀',
                (false, true) => '▄',
                (true, true) => '█',
            });
        }
        out.push('\n');
    }
    Ok(out)
}
