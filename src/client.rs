//! 知乎 API 客户端（cookie 登录，V4 接口）。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use base64::Engine;
use hmac::{Hmac, Mac};
use md5::{Digest, Md5};
use reqwest::blocking::{Client, RequestBuilder};
use reqwest::cookie::{CookieStore, Jar};
use reqwest::Url;
use serde_json::{json, Value};
use sha1::Sha1;
use uuid::Uuid;

use crate::config::{
    browser_headers, DEFAULT_TIMEOUT_SECS, ZHIHU_API_V4, ZHIHU_CONTENT_DRAFTS_URL,
    ZHIHU_CONTENT_PUBLISH_URL, ZHIHU_IMAGE_API, ZHIHU_OSS_UPLOAD_URL, ZHIHU_ZHUANLAN_API,
};
use crate::display;

pub const ZHIHU_ORIGIN: &str = "https://www.zhihu.com/";

/// 构造一个共享 cookie jar 的 HTTP 客户端。
pub fn build_http(jar: Arc<Jar>) -> Result<Client> {
    Client::builder()
        .cookie_provider(jar)
        .default_headers(browser_headers())
        .timeout(Duration::from_secs(DEFAULT_TIMEOUT_SECS))
        .build()
        .context("初始化 HTTP 客户端失败")
}

pub fn new_jar() -> Arc<Jar> {
    Arc::new(Jar::default())
}

pub fn origin_url() -> Url {
    Url::parse(ZHIHU_ORIGIN).expect("valid url")
}

pub fn add_cookies(jar: &Jar, cookies: &HashMap<String, String>) {
    let url = origin_url();
    for (name, value) in cookies {
        jar.add_cookie_str(
            &format!("{}={}; Domain=.zhihu.com; Path=/", name, value),
            &url,
        );
    }
}

pub fn parse_cookie_header(header: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for part in header.split(';') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=') {
            if !k.trim().is_empty() {
                out.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }
    out
}

pub fn jar_cookies(jar: &Jar) -> HashMap<String, String> {
    jar.cookies(&origin_url())
        .and_then(|v| v.to_str().ok().map(parse_cookie_header))
        .unwrap_or_default()
}

pub struct ZhihuClient {
    http: Client,
    jar: Arc<Jar>,
}

impl ZhihuClient {
    pub fn new(cookies: &HashMap<String, String>) -> Result<Self> {
        let jar = new_jar();
        add_cookies(&jar, cookies);
        let http = build_http(jar.clone())?;
        Ok(Self { http, jar })
    }

    pub fn cookies(&self) -> HashMap<String, String> {
        jar_cookies(&self.jar)
    }

    fn xsrf(&self) -> Option<String> {
        self.cookies().get("_xsrf").cloned()
    }

    fn with_common_headers(&self, req: RequestBuilder) -> RequestBuilder {
        let req = req.header("x-requested-with", "fetch");
        match self.xsrf() {
            Some(xsrf) => req.header("x-xsrftoken", xsrf),
            None => req,
        }
    }

    fn send_json(&self, req: RequestBuilder) -> Result<Value> {
        let resp = req.send().context("请求失败")?;
        let status = resp.status();
        if status.as_u16() == 401 {
            bail!("登录态已失效，请重新登录（zhihu login --qrcode）");
        }
        if status.as_u16() == 403 {
            bail!(
                "访问被拒绝（403）：可能是登录态失效、请求过于频繁或接口风控，请稍后重试或重新登录"
            );
        }
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("接口请求失败 ({status}): {}", display::truncate(&body, 200));
        }
        resp.json::<Value>().context("响应不是合法 JSON")
    }

    fn get_json(&self, url: &str, query: &[(&str, String)]) -> Result<Value> {
        let mut req = self.with_common_headers(self.http.get(url));
        if !query.is_empty() {
            req = req.query(query);
        }
        self.send_json(req)
    }

    fn post_json(&self, url: &str, body: &Value) -> Result<Value> {
        self.send_json(self.with_common_headers(self.http.post(url).json(body)))
    }

    // ── 当前用户 ────────────────────────────────────────────────────────────

    pub fn get_self_info(&self) -> Result<Value> {
        self.get_json(&format!("{}/me", ZHIHU_API_V4), &[])
    }

    // ── 搜索 ────────────────────────────────────────────────────────────────

    pub fn search(
        &self,
        keyword: &str,
        search_type: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Value> {
        self.get_json(
            &format!("{}/search_v3", ZHIHU_API_V4),
            &[
                ("gk_version", "gz-gaokao".into()),
                ("t", search_type.into()),
                ("q", keyword.into()),
                ("correction", "1".into()),
                ("offset", offset.to_string()),
                ("limit", limit.to_string()),
                ("filter_fields", "lc_idx".into()),
                ("lc_idx", "0".into()),
                ("show_all_topics", "0".into()),
                ("search_source", "Normal".into()),
            ],
        )
    }

    // ── 热榜 ────────────────────────────────────────────────────────────────

    pub fn get_hot_list(&self, limit: usize) -> Result<Value> {
        let url = format!("{}/creators/rank/hot", ZHIHU_API_V4);
        let result = self.get_json(
            &url,
            &[("domain", "0".into()), ("limit", limit.to_string())],
        );
        if let Ok(v) = result {
            return Ok(v);
        }
        self.get_json(
            "https://www.zhihu.com/api/v3/feed/topstory/hot-lists/total",
            &[("limit", limit.to_string())],
        )
    }

    // ── 问题 / 回答 ─────────────────────────────────────────────────────────

    pub fn get_question(&self, question_id: &str) -> Result<Value> {
        self.get_json(
            &format!("{}/questions/{}", ZHIHU_API_V4, question_id),
            &[(
                "include",
                "author,answer_count,follower_count,visit_count,comment_count,created_time,updated_time,detail,topics".into(),
            )],
        )
    }

    pub fn get_question_answers(
        &self,
        question_id: &str,
        offset: usize,
        limit: usize,
        sort_by: &str,
    ) -> Result<Value> {
        self.get_json(
            &format!("{}/questions/{}/answers", ZHIHU_API_V4, question_id),
            &[
                (
                    "include",
                    "data[*].content,voteup_count,comment_count,created_time,updated_time,author"
                        .into(),
                ),
                ("offset", offset.to_string()),
                ("limit", limit.to_string()),
                ("sort_by", sort_by.into()),
            ],
        )
    }

    pub fn get_answer(&self, answer_id: &str) -> Result<Value> {
        self.get_json(
            &format!("{}/answers/{}", ZHIHU_API_V4, answer_id),
            &[(
                "include",
                "content,voteup_count,comment_count,created_time,updated_time,author,question"
                    .into(),
            )],
        )
    }

    // ── 用户 ────────────────────────────────────────────────────────────────

    pub fn get_user_profile(&self, url_token: &str) -> Result<Value> {
        self.get_json(
            &format!("{}/members/{}", ZHIHU_API_V4, url_token),
            &[(
                "include",
                "answer_count,articles_count,follower_count,following_count,voteup_count,thanked_count,favorite_count,favorited_count,gender,badge,description,business,educations,employments,locations".into(),
            )],
        )
    }

    pub fn get_user_answers(
        &self,
        url_token: &str,
        offset: usize,
        limit: usize,
        sort_by: &str,
    ) -> Result<Value> {
        self.get_json(
            &format!("{}/members/{}/answers", ZHIHU_API_V4, url_token),
            &[
                (
                    "include",
                    "data[*].content,voteup_count,comment_count,created_time,question".into(),
                ),
                ("offset", offset.to_string()),
                ("limit", limit.to_string()),
                ("sort_by", sort_by.into()),
            ],
        )
    }

    pub fn get_user_articles(
        &self,
        url_token: &str,
        offset: usize,
        limit: usize,
        sort_by: &str,
    ) -> Result<Value> {
        self.get_json(
            &format!("{}/members/{}/articles", ZHIHU_API_V4, url_token),
            &[
                (
                    "include",
                    "data[*].content,voteup_count,comment_count,created_time,updated_time".into(),
                ),
                ("offset", offset.to_string()),
                ("limit", limit.to_string()),
                ("sort_by", sort_by.into()),
            ],
        )
    }

    pub fn get_followers(&self, url_token: &str, offset: usize, limit: usize) -> Result<Value> {
        self.get_json(
            &format!("{}/members/{}/followers", ZHIHU_API_V4, url_token),
            &[
                (
                    "include",
                    "data[*].answer_count,articles_count,follower_count".into(),
                ),
                ("offset", offset.to_string()),
                ("limit", limit.to_string()),
            ],
        )
    }

    pub fn get_following(&self, url_token: &str, offset: usize, limit: usize) -> Result<Value> {
        self.get_json(
            &format!("{}/members/{}/followees", ZHIHU_API_V4, url_token),
            &[
                (
                    "include",
                    "data[*].answer_count,articles_count,follower_count".into(),
                ),
                ("offset", offset.to_string()),
                ("limit", limit.to_string()),
            ],
        )
    }

    // ── 推荐 / 话题 ─────────────────────────────────────────────────────────

    pub fn get_feed(&self, limit: usize) -> Result<Value> {
        self.get_json(
            "https://www.zhihu.com/api/v3/feed/topstory/recommend",
            &[
                ("page_number", "1".into()),
                ("limit", limit.to_string()),
                ("action", "down".into()),
            ],
        )
    }

    pub fn get_topic(&self, topic_id: &str) -> Result<Value> {
        self.get_json(&format!("{}/topics/{}", ZHIHU_API_V4, topic_id), &[])
    }

    pub fn get_topic_hot_questions(
        &self,
        topic_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Value> {
        self.get_json(
            &format!("{}/topics/{}/feeds/essence", ZHIHU_API_V4, topic_id),
            &[("offset", offset.to_string()), ("limit", limit.to_string())],
        )
    }

    // ── 评论 ────────────────────────────────────────────────────────────────

    pub fn get_answer_comments(
        &self,
        answer_id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Value> {
        self.get_json(
            &format!("{}/answers/{}/comments", ZHIHU_API_V4, answer_id),
            &[
                ("offset", offset.to_string()),
                ("limit", limit.to_string()),
                ("order", "normal".into()),
                ("status", "open".into()),
            ],
        )
    }

    // ── 互动 ────────────────────────────────────────────────────────────────

    pub fn vote(&self, answer_id: &str, kind: &str) -> Result<bool> {
        let url = format!("{}/answers/{}/voters", ZHIHU_API_V4, answer_id);
        let resp = self
            .with_common_headers(self.http.post(&url).json(&json!({ "type": kind })))
            .send()
            .context("投票请求失败")?;
        Ok(resp.status().is_success())
    }

    pub fn follow_question(&self, question_id: &str, follow: bool) -> Result<bool> {
        let url = format!("{}/questions/{}/followers", ZHIHU_API_V4, question_id);
        let req = if follow {
            self.http.post(&url)
        } else {
            self.http.delete(&url)
        };
        let resp = self
            .with_common_headers(req)
            .send()
            .context("关注请求失败")?;
        Ok(resp.status().is_success())
    }

    // ── 图片上传 ────────────────────────────────────────────────────────────

    pub fn upload_image(&self, file_path: &str, source: &str) -> Result<Value> {
        let path = std::path::Path::new(file_path);
        if !path.is_file() {
            bail!("图片文件不存在: {}", file_path);
        }
        let image_data = std::fs::read(path).context("读取图片失败")?;
        let md5_hex = format!("{:x}", Md5::digest(&image_data));

        let resp = self
            .with_common_headers(self.http.post(ZHIHU_IMAGE_API).json(&json!({
                "image_hash": md5_hex,
                "source": source,
            })))
            .send()
            .context("图片注册失败")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            bail!("图片注册失败 ({status}): {}", display::truncate(&body, 200));
        }
        let data: Value = resp.json().context("图片注册响应无效")?;
        let upload_file = data
            .get("upload_file")
            .context("图片注册响应缺少 upload_file")?;
        let image_id = upload_file
            .get("image_id")
            .and_then(|v| {
                v.as_i64()
                    .map(|n| n.to_string())
                    .or_else(|| v.as_str().map(str::to_string))
            })
            .context("图片注册响应缺少 image_id")?;
        let state = upload_file
            .get("state")
            .and_then(Value::as_i64)
            .unwrap_or(0);

        if state == 2 {
            let obj_key = upload_file
                .get("object_key")
                .and_then(Value::as_str)
                .context("图片注册响应缺少 object_key")?;
            let token = data.get("upload_token").context("缺少 upload_token")?;
            self.upload_to_oss(obj_key, &image_data, token)?;
        } else if state != 1 {
            bail!("未知的图片状态: {state}");
        }

        let mut info = self.poll_image(&image_id)?;

        if let Ok((w, h)) = image::image_dimensions(path) {
            info["width"] = json!(w);
            info["height"] = json!(h);
        } else {
            info["width"] = json!(0);
            info["height"] = json!(0);
        }
        Ok(info)
    }

    fn upload_to_oss(&self, obj_key: &str, data: &[u8], token: &Value) -> Result<()> {
        let content_type = "image/jpeg";
        let date = httpdate::fmt_http_date(std::time::SystemTime::now());
        let security_token = token
            .get("access_token")
            .and_then(Value::as_str)
            .context("upload_token 缺少 access_token")?;
        let access_id = token
            .get("access_id")
            .and_then(Value::as_str)
            .context("upload_token 缺少 access_id")?;
        let access_key = token
            .get("access_key")
            .and_then(Value::as_str)
            .context("upload_token 缺少 access_key")?;

        let string_to_sign = format!(
            "PUT\n\n{}\n{}\nx-oss-security-token:{}\n/zhihu-pics/{}",
            content_type, date, security_token, obj_key
        );
        type HmacSha1 = Hmac<Sha1>;
        let mut mac = HmacSha1::new_from_slice(access_key.as_bytes())
            .map_err(|e| anyhow::anyhow!("HMAC 初始化失败: {e}"))?;
        mac.update(string_to_sign.as_bytes());
        let signature =
            base64::engine::general_purpose::STANDARD.encode(mac.finalize().into_bytes());

        let resp = Client::new()
            .put(format!("{}/{}", ZHIHU_OSS_UPLOAD_URL, obj_key))
            .header("Content-Type", content_type)
            .header("Date", date)
            .header("x-oss-security-token", security_token)
            .header("Authorization", format!("OSS {}:{}", access_id, signature))
            .body(data.to_vec())
            .send()
            .context("OSS 上传失败")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            bail!("OSS 上传失败 ({status}): {}", display::truncate(&body, 200));
        }
        Ok(())
    }

    fn poll_image(&self, image_id: &str) -> Result<Value> {
        for _ in 0..15 {
            let resp = self
                .with_common_headers(self.http.get(format!("{}/{}", ZHIHU_IMAGE_API, image_id)))
                .send()
                .context("图片状态查询失败")?;
            if !resp.status().is_success() {
                let status = resp.status();
                let body = resp.text().unwrap_or_default();
                bail!(
                    "图片状态查询失败 ({status}): {}",
                    display::truncate(&body, 200)
                );
            }
            let data: Value = resp.json().context("图片状态响应无效")?;
            if data.get("status").and_then(Value::as_str) == Some("success") {
                return Ok(json!({
                    "src": data.get("src").cloned().unwrap_or(Value::Null),
                    "original_src": data.get("original_src").cloned().unwrap_or(Value::Null),
                    "watermark": data.get("watermark").cloned().unwrap_or(json!("watermark")),
                    "watermark_src": data.get("watermark_src").cloned().unwrap_or(json!("")),
                }));
            }
            std::thread::sleep(Duration::from_secs(2));
        }
        bail!("图片处理超时")
    }

    fn build_img_html(image_infos: &[Value]) -> String {
        let mut tags = String::new();
        for info in image_infos {
            let src = info.get("src").and_then(Value::as_str).unwrap_or("");
            let original = info
                .get("original_src")
                .and_then(Value::as_str)
                .unwrap_or(src);
            let watermark = info
                .get("watermark")
                .and_then(Value::as_str)
                .unwrap_or("watermark");
            let watermark_src = info
                .get("watermark_src")
                .and_then(Value::as_str)
                .unwrap_or("");
            let w = info.get("width").and_then(Value::as_u64).unwrap_or(0);
            let h = info.get("height").and_then(Value::as_u64).unwrap_or(0);
            tags.push_str(&format!(
                "<img src=\"{src}\" data-caption=\"\" data-size=\"normal\" data-rawwidth=\"{w}\" data-rawheight=\"{h}\" data-watermark=\"{watermark}\" data-original-src=\"{original}\" data-watermark-src=\"{watermark_src}\" data-private-watermark-src=\"\"/>"
            ));
        }
        tags
    }

    fn create_content_draft(&self, action: &str) -> Result<String> {
        let data = self.post_json(ZHIHU_CONTENT_DRAFTS_URL, &json!({ "action": action }))?;
        let content_id = data
            .pointer("/data/content_id")
            .and_then(|v| {
                v.as_str()
                    .map(str::to_string)
                    .or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .filter(|s| !s.is_empty())
            .context("草稿创建成功但未返回 content_id")?;
        Ok(content_id)
    }

    fn content_publish(&self, payload: &Value) -> Result<Value> {
        let resp = self
            .with_common_headers(self.http.post(ZHIHU_CONTENT_PUBLISH_URL).json(payload))
            .send()
            .context("发布请求失败")?;
        let status = resp.status();
        if status.as_u16() == 401 {
            bail!("登录态已失效，请重新登录");
        }
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("发布失败 ({status}): {}", display::truncate(&body, 200));
        }
        let data: Value = resp.json().unwrap_or_else(|_| json!({}));
        if let Some(code) = data.get("code").and_then(Value::as_i64) {
            if code != 0 {
                let msg = data
                    .get("message")
                    .or_else(|| data.get("toast_message"))
                    .and_then(Value::as_str)
                    .unwrap_or("未知错误");
                bail!("发布失败: {msg}");
            }
        }
        if let Some(result) = data.pointer("/data/result").and_then(Value::as_str) {
            if !result.is_empty() {
                if let Ok(parsed) = serde_json::from_str::<Value>(result) {
                    return Ok(parsed);
                }
            }
        }
        Ok(data)
    }

    // ── 发布提问 / 想法 / 文章 ──────────────────────────────────────────────

    pub fn create_question(
        &self,
        title: &str,
        detail: &str,
        topic_ids: &[String],
        image_infos: Option<&[Value]>,
    ) -> Result<Value> {
        if let Some(images) = image_infos {
            let html = format!("{}{}", detail, Self::build_img_html(images));
            let payload = json!({
                "action": "question",
                "data": {
                    "title": { "title": title },
                    "topic": { "topics": topic_ids },
                    "hybrid": { "html": html, "textLength": detail.chars().count() },
                    "extra_info": { "publisher": "pc" },
                    "questionConfig": { "type": "0" },
                    "draft": { "disabled": 1 },
                }
            });
            return self.content_publish(&payload);
        }

        let mut payload = json!({
            "title": title,
            "detail": detail,
        });
        if !topic_ids.is_empty() {
            payload["topic_url_tokens"] = json!(topic_ids);
        }
        self.post_json(&format!("{}/questions", ZHIHU_API_V4), &payload)
    }

    pub fn create_pin(
        &self,
        title: &str,
        content: &str,
        image_infos: Option<&[Value]>,
    ) -> Result<Value> {
        if let Some(images) = image_infos {
            return self.create_pin_with_images(title, content, images);
        }

        let draft_id = self.create_content_draft("pin")?;
        let body_html = if content.trim().is_empty() {
            String::new()
        } else {
            format!("<p>{}</p>", content.trim())
        };
        let payload = json!({
            "action": "pin",
            "data": {
                "publish": { "traceId": format!("{},{}", now_millis(), Uuid::new_v4()) },
                "commentsPermission": { "comment_permission": "all" },
                "extra_info": { "view_permission": "all", "publisher": "pc" },
                "draft": { "disabled": 1, "id": draft_id },
                "title": { "title": title },
                "hybrid": { "html": body_html, "textLength": content.trim().chars().count() },
            }
        });
        self.content_publish(&payload)
    }

    fn create_pin_with_images(
        &self,
        title: &str,
        content: &str,
        image_infos: &[Value],
    ) -> Result<Value> {
        let draft_id = self.create_content_draft("pin")?;
        let medias: Vec<Value> = image_infos
            .iter()
            .map(|info| {
                let src = info.get("src").cloned().unwrap_or(Value::Null);
                let original = info
                    .get("original_src")
                    .cloned()
                    .unwrap_or_else(|| src.clone());
                json!({
                    "image": {
                        "width": info.get("width").and_then(Value::as_u64).unwrap_or(0),
                        "height": info.get("height").and_then(Value::as_u64).unwrap_or(0),
                        "url": src,
                        "originalUrl": original,
                        "watermark": info.get("watermark").cloned().unwrap_or(json!("watermark")),
                        "watermarkUrl": info.get("watermark_src").cloned().unwrap_or(json!("")),
                    }
                })
            })
            .collect();
        let html = format!("{}{}", content, Self::build_img_html(image_infos));
        let payload = json!({
            "action": "pin",
            "data": {
                "publish": { "traceId": format!("{},{}", now_millis(), Uuid::new_v4()) },
                "commentsPermission": { "comment_permission": "all" },
                "extra_info": { "view_permission": "all", "publisher": "pc" },
                "draft": { "disabled": 1, "id": draft_id },
                "title": { "title": title },
                "hybrid": { "html": html, "textLength": title.chars().count() + content.chars().count() },
                "media": { "medias": medias },
            }
        });
        self.content_publish(&payload)
    }

    pub fn create_article(
        &self,
        title: &str,
        content: &str,
        topic_ids: &[String],
        image_infos: Option<&[Value]>,
    ) -> Result<Value> {
        if let Some(images) = image_infos {
            let html = format!("{}{}", content, Self::build_img_html(images));
            let draft_id = self.create_content_draft("article")?;
            let payload = json!({
                "action": "article",
                "data": {
                    "title": { "title": title },
                    "hybrid": { "html": html, "textLength": content.chars().count() },
                    "extra_info": { "publisher": "pc" },
                    "draft": { "disabled": 1, "id": draft_id },
                    "commentsPermission": { "comment_permission": "anyone" },
                }
            });
            return self.content_publish(&payload);
        }

        let base = ZHIHU_ZHUANLAN_API;
        let draft = self.post_json(&format!("{}/articles/drafts", base), &json!({}))?;
        let draft_id = draft
            .get("id")
            .and_then(|v| {
                v.as_str()
                    .map(str::to_string)
                    .or_else(|| v.as_i64().map(|n| n.to_string()))
            })
            .filter(|s| !s.is_empty())
            .context("草稿创建成功但未返回 ID")?;

        let mut patch = json!({ "title": title, "content": content });
        if !topic_ids.is_empty() {
            patch["topics"] = json!(topic_ids);
        }
        let resp = self
            .with_common_headers(
                self.http
                    .patch(format!("{}/articles/{}/draft", base, draft_id))
                    .json(&patch),
            )
            .send()
            .context("更新文章草稿失败")?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().unwrap_or_default();
            bail!(
                "更新文章草稿失败 ({status}): {}",
                display::truncate(&body, 200)
            );
        }

        let resp = self
            .with_common_headers(
                self.http
                    .put(format!("{}/articles/{}/publish", base, draft_id))
                    .json(&json!({ "column": null, "commentPermission": "anyone" })),
            )
            .send()
            .context("发布文章失败")?;
        let status = resp.status();
        if status.as_u16() == 401 {
            bail!("登录态已失效，请重新登录");
        }
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("发布文章失败 ({status}): {}", display::truncate(&body, 200));
        }
        Ok(resp.json().unwrap_or_else(|_| json!({})))
    }

    // ── 删除自己的内容 ──────────────────────────────────────────────────────

    pub fn delete_question(&self, question_id: &str) -> Result<bool> {
        self.delete(
            &format!("{}/questions/{}", ZHIHU_API_V4, question_id),
            "提问",
        )
    }

    pub fn delete_pin(&self, pin_id: &str) -> Result<bool> {
        self.delete(&format!("{}/pins/{}", ZHIHU_API_V4, pin_id), "想法")
    }

    pub fn delete_article(&self, article_id: &str) -> Result<bool> {
        self.delete(
            &format!("{}/articles/{}", ZHIHU_ZHUANLAN_API, article_id),
            "文章",
        )
    }

    fn delete(&self, url: &str, label: &str) -> Result<bool> {
        let resp = self
            .with_common_headers(self.http.delete(url))
            .send()
            .context("删除请求失败")?;
        let status = resp.status();
        if status.as_u16() == 401 {
            bail!("登录态已失效，请重新登录");
        }
        if status.as_u16() == 403 {
            bail!("没有权限删除该{label}");
        }
        Ok(status.is_success())
    }

    // ── 收藏 / 通知 ─────────────────────────────────────────────────────────

    pub fn get_collections(&self, offset: usize, limit: usize) -> Result<Value> {
        let me = self.get_self_info()?;
        let url_token = me.get("url_token").and_then(Value::as_str).unwrap_or("");
        if url_token.is_empty() {
            bail!("无法获取用户信息，请确认登录状态");
        }
        self.get_json(
            &format!("{}/members/{}/favlists", ZHIHU_API_V4, url_token),
            &[("offset", offset.to_string()), ("limit", limit.to_string())],
        )
    }

    pub fn get_notifications(
        &self,
        limit: usize,
        offset: usize,
        entry_name: &str,
    ) -> Result<Value> {
        self.get_json(
            &format!("{}/notifications/v2/recent", ZHIHU_API_V4),
            &[
                ("limit", limit.to_string()),
                ("entry_name", entry_name.into()),
                ("offset", offset.to_string()),
            ],
        )
    }
}

fn now_millis() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0)
}
