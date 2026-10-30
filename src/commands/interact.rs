//! 互动与创作命令：赞同、关注、发布、删除、收藏、通知。

use std::io::{self, Write};

use anyhow::{bail, Result};
use serde_json::Value;

use crate::commands::{client, print_json, value_to_id};
use crate::display;
use crate::html::html_to_inline;
use crate::util::{as_array, json_bool, json_str, ptr_str};

pub fn vote(answer_id: &str, neutral: bool) -> Result<()> {
    let client = client()?;
    let kind = if neutral { "neutral" } else { "up" };
    if client.vote(answer_id, kind)? {
        if neutral {
            display::success(&format!("已取消赞同回答 {}", answer_id));
        } else {
            display::success(&format!("已赞同回答 {}", answer_id));
        }
    } else {
        bail!("操作未被服务器接受");
    }
    Ok(())
}

pub fn follow_question(question_id: &str, unfollow: bool) -> Result<()> {
    let client = client()?;
    if client.follow_question(question_id, !unfollow)? {
        if unfollow {
            display::success(&format!("已取消关注问题 {}", question_id));
        } else {
            display::success(&format!("已关注问题 {}", question_id));
        }
    } else {
        bail!("操作未被服务器接受");
    }
    Ok(())
}

pub fn collections(limit: usize, as_json: bool) -> Result<()> {
    let client = client()?;
    let results = client.get_collections(0, limit)?;
    if as_json {
        print_json(&results);
        return Ok(());
    }
    let data = as_array(results.get("data"));
    if data.is_empty() {
        display::info("没有收藏夹");
        return Ok(());
    }
    let rows = data
        .iter()
        .enumerate()
        .map(|(i, col)| {
            let count = col.get("item_count").or_else(|| col.get("answer_count"));
            vec![
                (i + 1).to_string(),
                json_str(col, "title").to_string(),
                display::format_count(count),
            ]
        })
        .collect();
    display::print_table(Some("我的收藏夹"), &["#", "标题", "内容数"], rows, &[0, 2]);
    Ok(())
}

fn format_notification_line(n: &Value) -> String {
    let content = n.get("content").cloned().unwrap_or(Value::Null);
    let actors = as_array(content.get("actors"));
    let verb = json_str(&content, "verb").trim();
    let target_text = html_to_inline(ptr_str(&content, "/target/text"));
    let names = actors
        .iter()
        .filter_map(|a| a.get("name").and_then(Value::as_str))
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(", ");

    let mut line = if !names.is_empty() && !verb.is_empty() {
        format!("{} {}", names, verb)
    } else if !target_text.is_empty() {
        target_text.clone()
    } else {
        verb.to_string()
    };
    if !target_text.is_empty() && line != target_text {
        line = format!("{} · {}", line, target_text);
    }
    if line.trim().is_empty() {
        "—".to_string()
    } else {
        line.trim().to_string()
    }
}

pub fn notifications(limit: usize, offset: usize, as_json: bool) -> Result<()> {
    let client = client()?;
    let results = client.get_notifications(limit, offset, "all")?;
    if as_json {
        print_json(&results);
        return Ok(());
    }
    let data = as_array(results.get("data"));
    if data.is_empty() {
        display::info("没有通知");
        return Ok(());
    }
    let rows = data
        .iter()
        .enumerate()
        .map(|(i, n)| {
            vec![
                (i + 1).to_string(),
                if json_bool(n, "is_read") { "✓" } else { "·" }.to_string(),
                format_notification_line(n),
            ]
        })
        .collect();
    display::print_table(Some("通知"), &["#", "已读", "内容"], rows, &[0, 1]);

    let is_end = results
        .pointer("/paging/is_end")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let next = ptr_str(&results, "/paging/next");
    if !is_end && !next.is_empty() {
        if let Ok(url) = reqwest::Url::parse(next) {
            if let Some((_, value)) = url.query_pairs().find(|(k, _)| k == "offset") {
                display::hint(&format!(
                    "下一页: zhihu notifications --offset {} -l {}",
                    value, limit
                ));
            }
        }
    }
    Ok(())
}

fn upload_images(
    client: &crate::client::ZhihuClient,
    paths: &[String],
    source: &str,
) -> Result<Vec<Value>> {
    let mut infos = Vec::new();
    for path in paths {
        display::info(&format!("上传图片: {}", path));
        infos.push(client.upload_image(path, source)?);
    }
    Ok(infos)
}

pub fn ask(title: &str, detail: &str, topics: &[String], images: &[String]) -> Result<()> {
    if title.trim().is_empty() {
        bail!("标题不能为空");
    }
    let client = client()?;
    let image_infos = if images.is_empty() {
        None
    } else {
        Some(upload_images(&client, images, "question")?)
    };
    let result = client.create_question(title.trim(), detail, topics, image_infos.as_deref())?;
    let qid = value_to_id(result.get("id"));
    if qid.is_empty() {
        display::warning("提问可能已创建，但接口未返回 ID");
    } else {
        display::success(&format!(
            "提问已发布！ID: {}\n  https://www.zhihu.com/question/{}",
            qid, qid
        ));
    }
    Ok(())
}

pub fn pin(title: &str, content: &str, images: &[String]) -> Result<()> {
    if title.trim().is_empty() {
        bail!("标题不能为空");
    }
    let client = client()?;
    let image_infos = if images.is_empty() {
        None
    } else {
        Some(upload_images(&client, images, "pin")?)
    };
    let result = client.create_pin(title.trim(), content.trim(), image_infos.as_deref())?;
    let pid = value_to_id(result.get("id"));
    if pid.is_empty() {
        display::warning("想法可能已发布，但接口未返回 ID");
    } else {
        display::success(&format!(
            "想法已发布！ID: {}\n  https://www.zhihu.com/pin/{}",
            pid, pid
        ));
    }
    Ok(())
}

fn looks_like_html_block(content: &str) -> bool {
    let lower = content.to_ascii_lowercase();
    [
        "<p",
        "<div",
        "<h1",
        "<h2",
        "<h3",
        "<h4",
        "<h5",
        "<h6",
        "<ul",
        "<ol",
        "<blockquote",
        "<pre",
        "<figure",
        "<table",
    ]
    .iter()
    .any(|tag| lower.contains(tag))
}

pub fn article(title: &str, content: &str, topics: &[String], images: &[String]) -> Result<()> {
    if title.trim().is_empty() {
        bail!("标题不能为空");
    }
    if content.trim().is_empty() {
        bail!("正文不能为空");
    }
    let client = client()?;
    let trimmed = content.trim();
    let body = if looks_like_html_block(trimmed) {
        trimmed.to_string()
    } else {
        format!("<p>{}</p>", trimmed)
    };
    let image_infos = if images.is_empty() {
        None
    } else {
        Some(upload_images(&client, images, "article")?)
    };
    let result = client.create_article(title.trim(), &body, topics, image_infos.as_deref())?;
    let aid = value_to_id(result.get("id"));
    if aid.is_empty() {
        display::warning("文章可能已发布，但接口未返回 ID");
    } else {
        display::success(&format!(
            "文章已发布！ID: {}\n  https://zhuanlan.zhihu.com/p/{}",
            aid, aid
        ));
    }
    Ok(())
}

fn confirm(prompt: &str) -> Result<()> {
    print!("{} [y/N] ", prompt);
    io::stdout().flush().ok();
    let mut input = String::new();
    io::stdin().read_line(&mut input).ok();
    let answer = input.trim().to_lowercase();
    if answer == "y" || answer == "yes" {
        Ok(())
    } else {
        bail!("已取消")
    }
}

pub fn delete_question(id: &str, yes: bool) -> Result<()> {
    if !yes {
        confirm(&format!("确认删除提问 {}？该操作不可撤销。", id))?;
    }
    let client = client()?;
    if client.delete_question(id)? {
        display::success(&format!("提问 {} 已删除", id));
        Ok(())
    } else {
        bail!("服务器未接受删除请求")
    }
}

pub fn delete_pin(id: &str, yes: bool) -> Result<()> {
    if !yes {
        confirm(&format!("确认删除想法 {}？该操作不可撤销。", id))?;
    }
    let client = client()?;
    if client.delete_pin(id)? {
        display::success(&format!("想法 {} 已删除", id));
        Ok(())
    } else {
        bail!("服务器未接受删除请求")
    }
}

pub fn delete_article(id: &str, yes: bool) -> Result<()> {
    if !yes {
        confirm(&format!("确认删除文章 {}？该操作不可撤销。", id))?;
    }
    let client = client()?;
    if client.delete_article(id)? {
        display::success(&format!("文章 {} 已删除", id));
        Ok(())
    } else {
        bail!("服务器未接受删除请求")
    }
}
