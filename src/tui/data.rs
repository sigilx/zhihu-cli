//! TUI 的数据加载：把知乎 API 的 JSON 转成列表项和正文文档。
//!
//! 所有函数都是阻塞的，由后台线程调用，避免卡住界面。

use anyhow::{bail, Result};
use serde_json::Value;

use crate::client::ZhihuClient;
use crate::commands::value_to_id;
use crate::display;
use crate::html::html_to_inline;
use crate::util::{as_array, json_str, ptr_str};

use super::app::{Detail, ItemKind, ListItem, Tab};

fn kind_from(type_name: &str) -> ItemKind {
    match type_name {
        "answer" => ItemKind::Answer,
        "question" => ItemKind::Question,
        "article" => ItemKind::Article,
        "pin" => ItemKind::Pin,
        "people" | "member" => ItemKind::People,
        "topic" => ItemKind::Topic,
        other => ItemKind::Other(other.to_string()),
    }
}

fn inline(value: &str) -> String {
    html_to_inline(value)
}

fn counts_meta(value: &Value) -> String {
    let mut parts = Vec::new();
    if let Some(v) = value
        .get("voteup_count")
        .or_else(|| value.get("vote_count"))
    {
        parts.push(format!("{} 赞同", display::format_count(Some(v))));
    }
    if let Some(v) = value.get("comment_count") {
        parts.push(format!("{} 评论", display::format_count(Some(v))));
    }
    if let Some(v) = value.get("answer_count") {
        parts.push(format!("{} 回答", display::format_count(Some(v))));
    }
    if let Some(v) = value.get("follower_count") {
        parts.push(format!("{} 关注", display::format_count(Some(v))));
    }
    parts.join(" · ")
}

/// 加载某个页签的列表。
pub fn load_list(client: &ZhihuClient, tab: Tab, query: &str) -> Result<Vec<ListItem>> {
    match tab {
        Tab::Hot => load_hot(client),
        Tab::Feed => load_feed(client),
        Tab::Search => load_search(client, query),
    }
}

fn load_hot(client: &ZhihuClient) -> Result<Vec<ListItem>> {
    let result = client.get_hot_list(50)?;
    let items: Vec<ListItem> = as_array(result.get("data"))
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let target = item
                .get("target")
                .or_else(|| item.get("question"))
                .unwrap_or(item);
            let id = value_to_id(target.get("id"));
            let title = inline(json_str(target, "title"));
            if id.is_empty() || title.is_empty() {
                return None;
            }
            let heat = if !json_str(item, "detail_text").is_empty() {
                inline(json_str(item, "detail_text"))
            } else {
                let pv = item
                    .pointer("/reaction/pv")
                    .or_else(|| item.pointer("/reaction/new_pv"))
                    .and_then(Value::as_i64)
                    .unwrap_or(0);
                if pv > 0 {
                    format!("{} 浏览", display::fmt_count(pv))
                } else {
                    String::new()
                }
            };
            Some(ListItem {
                id,
                kind: ItemKind::Question,
                title,
                author: ptr_str(target, "/author/name").to_string(),
                meta: format!("#{}  {}", index + 1, heat).trim_end().to_string(),
                excerpt: inline(json_str(target, "excerpt")),
                raw: target.clone(),
            })
        })
        .collect();
    Ok(items)
}

fn load_feed(client: &ZhihuClient) -> Result<Vec<ListItem>> {
    let result = client.get_feed(20)?;
    let items: Vec<ListItem> = as_array(result.get("data"))
        .iter()
        .filter_map(|item| {
            let target = item.get("target").unwrap_or(item);
            let type_name = json_str(target, "type");
            let kind = kind_from(type_name);
            let id = value_to_id(target.get("id"));
            let question_title = ptr_str(target, "/question/title");
            let raw_title = if !json_str(target, "title").is_empty() {
                json_str(target, "title")
            } else if !question_title.is_empty() {
                question_title
            } else {
                json_str(target, "excerpt")
            };
            let title = inline(raw_title);
            if id.is_empty() || title.is_empty() {
                return None;
            }
            let author = {
                let name = ptr_str(target, "/author/name");
                if name.is_empty() {
                    "匿名用户".to_string()
                } else {
                    name.to_string()
                }
            };
            let counts = counts_meta(target);
            Some(ListItem {
                id,
                kind,
                title,
                author,
                meta: counts,
                excerpt: inline(json_str(target, "excerpt")),
                raw: target.clone(),
            })
        })
        .collect();
    Ok(items)
}

fn load_search(client: &ZhihuClient, query: &str) -> Result<Vec<ListItem>> {
    if query.trim().is_empty() {
        return Ok(Vec::new());
    }
    let result = client.search(query, "general", 0, 20)?;
    let items: Vec<ListItem> = as_array(result.get("data"))
        .iter()
        .filter_map(|item| {
            let obj = item.get("object").unwrap_or(item);
            let wrapper_type = json_str(item, "type");
            let obj_type = json_str(obj, "type");
            let type_name = if wrapper_type == "search_result" && !obj_type.is_empty() {
                obj_type
            } else {
                wrapper_type
            };
            let kind = kind_from(type_name);
            let id = value_to_id(obj.get("id"));
            let raw_title = if !json_str(obj, "title").is_empty() {
                json_str(obj, "title")
            } else if !json_str(obj, "name").is_empty() {
                json_str(obj, "name")
            } else {
                json_str(obj, "excerpt")
            };
            let title = inline(raw_title);
            if id.is_empty() || title.is_empty() {
                return None;
            }
            let author = {
                let name = ptr_str(obj, "/author/name");
                if name.is_empty() {
                    ptr_str(obj, "/headline").to_string()
                } else {
                    name.to_string()
                }
            };
            let counts = counts_meta(obj);
            Some(ListItem {
                id,
                kind,
                title,
                author,
                meta: counts,
                excerpt: inline(json_str(obj, "excerpt")),
                raw: obj.clone(),
            })
        })
        .collect();
    Ok(items)
}

/// 加载选中项的完整内容。
pub fn load_detail(client: &ZhihuClient, item: &ListItem) -> Result<Detail> {
    match &item.kind {
        ItemKind::Question => {
            // `/questions/{id}` 偶尔会被风控（403），但回答接口通常仍可用。
            // 此时用列表里已有的问题数据兜底，先让用户读到回答。
            let question_result = client.get_question(&item.id);
            let answers_result = client.get_question_answers(&item.id, 0, 10, "default");
            let answers = match answers_result {
                Ok(value) => as_array(value.get("data")).to_vec(),
                Err(_) => Vec::new(),
            };
            let question = match question_result {
                Ok(question) => question,
                Err(question_error) => {
                    if answers.is_empty() {
                        bail!(
                            "读取问题失败：{}（可稍后重试，或先用 zhihu answers {}）",
                            question_error,
                            item.id
                        );
                    }
                    item.raw.clone()
                }
            };
            Ok(Detail::Question { question, answers })
        }
        ItemKind::Answer => {
            let answer = client.get_answer(&item.id)?;
            let comments = client
                .get_answer_comments(&item.id, 0, 20)
                .map(|v| as_array(v.get("data")).to_vec())
                .unwrap_or_default();
            Ok(Detail::Answer { answer, comments })
        }
        ItemKind::People => {
            let token = {
                let from_raw = json_str(&item.raw, "url_token");
                if from_raw.is_empty() {
                    item.id.clone()
                } else {
                    from_raw.to_string()
                }
            };
            if token.is_empty() {
                bail!("无法确定用户 url_token");
            }
            let user = client.get_user_profile(&token)?;
            Ok(Detail::Profile { user })
        }
        ItemKind::Topic => {
            let topic = client.get_topic(&item.id)?;
            Ok(Detail::Topic { topic })
        }
        kind => {
            let html = if !json_str(&item.raw, "content").is_empty() {
                json_str(&item.raw, "content").to_string()
            } else if !json_str(&item.raw, "excerpt").is_empty() {
                json_str(&item.raw, "excerpt").to_string()
            } else {
                item.excerpt.clone()
            };
            Ok(Detail::Raw {
                title: item.title.clone(),
                author: item.author.clone(),
                meta: item.meta.clone(),
                html,
                kind: kind.clone(),
            })
        }
    }
}
