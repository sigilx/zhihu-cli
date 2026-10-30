//! 全局配置：路径、URL、浏览器指纹。

use std::path::PathBuf;

use reqwest::header::{HeaderMap, HeaderValue};

pub const ZHIHU_BASE_URL: &str = "https://www.zhihu.com";
pub const ZHIHU_API_V4: &str = "https://www.zhihu.com/api/v4";
pub const ZHIHU_ZHUANLAN_API: &str = "https://zhuanlan.zhihu.com/api";
pub const ZHIHU_CONTENT_PUBLISH_URL: &str = "https://www.zhihu.com/api/v4/content/publish";
pub const ZHIHU_CONTENT_DRAFTS_URL: &str = "https://www.zhihu.com/api/v4/content/drafts";
pub const ZHIHU_IMAGE_API: &str = "https://api.zhihu.com/images";
pub const ZHIHU_OSS_UPLOAD_URL: &str = "https://zhihu-pics-upload.zhimg.com";
pub const ZHIHU_LOGIN_URL: &str = "https://www.zhihu.com/signin";
pub const ZHIHU_QRCODE_API: &str = "https://www.zhihu.com/api/v3/account/api/login/qrcode";
pub const ZHIHU_OAUTH_CAPTCHA: &str =
    "https://www.zhihu.com/api/v3/oauth/captcha/v2?type=captcha_sign_in";

pub const DEFAULT_TIMEOUT_SECS: u64 = 15;
pub const CHROME_VERSION: &str = "145";

/// 必需 Cookie：z_c0 登录态，_xsrf CSRF，d_c0 设备标识。
pub const REQUIRED_COOKIES: [&str; 3] = ["z_c0", "_xsrf", "d_c0"];

pub fn config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".zhihu-cli")
}

pub fn cookie_file() -> PathBuf {
    config_dir().join("cookies.json")
}

pub fn qrcode_image_path() -> PathBuf {
    config_dir().join("login_qrcode.png")
}

pub fn user_agent() -> String {
    format!(
        "Mozilla/5.0 (Windows NT 10.0; Win64; x64) \
         AppleWebKit/537.36 (KHTML, like Gecko) \
         Chrome/{}.0.0.0 Safari/537.36",
        CHROME_VERSION
    )
}

/// 与浏览器一致的请求头，降低风控概率。
pub fn browser_headers() -> HeaderMap {
    let mut headers = HeaderMap::new();
    let mut insert = |name: &'static str, value: String| {
        if let Ok(v) = HeaderValue::from_str(&value) {
            headers.insert(name, v);
        }
    };
    insert("user-agent", user_agent());
    insert("accept", "application/json, text/plain, */*".into());
    insert("accept-language", "en-US,en;q=0.9".into());
    insert("referer", format!("{}/", ZHIHU_BASE_URL));
    insert(
        "sec-ch-ua",
        format!(
            "\"Not:A-Brand\";v=\"99\", \"Google Chrome\";v=\"{v}\", \"Chromium\";v=\"{v}\"",
            v = CHROME_VERSION
        ),
    );
    insert("sec-ch-ua-mobile", "?0".into());
    insert("sec-ch-ua-platform", "\"Windows\"".into());
    headers
}
