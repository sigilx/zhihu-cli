//! 阅读类命令：搜索、热榜、问题、回答、推荐、话题。

use anyhow::Result;
use serde_json::Value;
use unicode_width::UnicodeWidthStr;

use crate::commands::{client, print_json, value_to_id};
use crate::display;
use crate::html::{self, html_to_inline, render_html};
use crate::util::{as_array, json_str, ptr_str};

fn render_body(html: &str, indent: usize) -> String {
    render_html(html, display::content_width().saturating_sub(indent))
}

fn print_body(rendered: &str, indent: usize) {
    let pad = " ".repeat(indent);
    for line in rendered.lines() {
        if line.is_empty() {
            println!();
        } else {
            println!("{}{}", pad, line);
        }
    }
}

fn print_hanging(prefix: &str, text: &str, suffix: &str) {
    let indent = prefix.width();
    let width = display::content_width().saturating_sub(indent).max(12);
    let mut first = true;
    for line in html::wrap_line(text, width) {
        if first {
            print!("{}{}", prefix, line);
            first = false;
        } else {
            print!("{}{}", " ".repeat(indent), line);
        }
        println!();
    }
    if !suffix.is_empty() {
        println!("{}{}", " ".repeat(indent), display::dim(suffix));
    }
}

fn print_heading(prefix: &str, title: &str) {
    let indent = prefix.width();
    let width = display::content_width().saturating_sub(indent).max(12);
    for (i, line) in html::wrap_line(title, width).iter().enumerate() {
        if i == 0 {
            print!("{}", prefix);
        } else {
            print!("{}", " ".repeat(indent));
        }
        println!("{}", display::title(line));
    }
}

fn print_comment(index: usize, content: &str, likes: &str) {
    let marker = format!("{}. ", index);
    let indent = 4 + marker.chars().count();
    let width = display::content_width().saturating_sub(indent).max(12);
    let mut first = true;
    for line in html::wrap_line(content, width) {
        if first {
            println!("    {}{}", display::dim(&marker), line);
            first = false;
        } else {
            println!("{}{}", " ".repeat(indent), line);
        }
    }
    println!(
        "{}{}",
        " ".repeat(indent),
        display::dim(&format!("{} 赞", likes))
    );
}

pub fn search(
    query: &str,
    search_type: &str,
    limit: usize,
    answers: usize,
    as_json: bool,
) -> Result<()> {
    let client = client()?;
    let results = client.search(query, search_type, 0, limit)?;
    if as_json {
        print_json(&results);
        return Ok(());
    }
    let data = as_array(results.get("data"));
    if data.is_empty() {
        display::info(&format!("没有找到与「{}」相关的结果", query));
        return Ok(());
    }

    let mut shown = 0;
    for item in data.iter() {
        let obj = item.get("object").unwrap_or(item);
        let wrapper_type = item.get("type").and_then(Value::as_str).unwrap_or("—");
        let obj_type = obj.get("type").and_then(Value::as_str).unwrap_or("");
        let label_type = if wrapper_type == "search_result" && !obj_type.is_empty() {
            obj_type
        } else {
            wrapper_type
        };
        let item_id = value_to_id(obj.get("id"));
        let raw_title = if !json_str(obj, "title").is_empty() {
            json_str(obj, "title")
        } else {
            json_str(obj, "name")
        };
        let title = html_to_inline(raw_title);
        if title.is_empty() {
            continue;
        }
        shown += 1;

        println!();
        print_heading(&format!("  {}. [{}] ", shown, label_type), &title);
        println!("  {}", display::dim(&format!("ID: {}", item_id)));

        if let Some(followers) = obj.get("follower_count") {
            println!("  {} followers", display::format_count(Some(followers)));
        } else if !json_str(obj, "excerpt").is_empty() {
            let excerpt = html_to_inline(json_str(obj, "excerpt"));
            print_hanging("    ", &display::truncate(&excerpt, 600), "");
        } else if let Some(answer_count) = obj.get("answer_count") {
            println!("  {} answers", display::format_count(Some(answer_count)));
        }

        if answers > 0
            && wrapper_type == "search_result"
            && matches!(obj_type, "answer" | "question")
            && !item_id.is_empty()
        {
            let q_id = obj
                .pointer("/question/id")
                .map(|v| value_to_id(Some(v)))
                .unwrap_or_else(|| item_id.clone());
            if let Ok(ans) = client.get_question_answers(&q_id, 0, answers, "default") {
                for a in as_array(ans.get("data")) {
                    let author = ptr_str(a, "/author/name");
                    let content = if !json_str(a, "excerpt").is_empty() {
                        html_to_inline(json_str(a, "excerpt"))
                    } else {
                        html_to_inline(json_str(a, "content"))
                    };
                    let upvotes = display::format_count(a.get("voteup_count"));
                    let content = display::truncate(&content, 800);
                    print_hanging(
                        &format!("    {}: ", author),
                        &content,
                        &format!("{} 赞", upvotes),
                    );
                }
            }
        }
    }
    println!();
    Ok(())
}

pub fn hot(limit: usize, answers: usize, as_json: bool) -> Result<()> {
    let client = client()?;
    let results = client.get_hot_list(limit)?;
    if as_json {
        print_json(&results);
        return Ok(());
    }
    let data = as_array(results.get("data"));
    if data.is_empty() {
        display::info("热榜为空");
        return Ok(());
    }

    for (idx, item) in data.iter().enumerate() {
        let target = item
            .get("target")
            .or_else(|| item.get("question"))
            .unwrap_or(item);
        let title = html_to_inline(json_str(target, "title"));
        let q_id = value_to_id(target.get("id"));
        let heat = if !json_str(item, "detail_text").is_empty() {
            html_to_inline(json_str(item, "detail_text"))
        } else {
            let pv = item
                .pointer("/reaction/pv")
                .or_else(|| item.pointer("/reaction/new_pv"))
                .and_then(Value::as_i64)
                .unwrap_or(0);
            if pv > 0 {
                format!("{} 浏览", display::fmt_count(pv))
            } else {
                "—".to_string()
            }
        };

        println!();
        print_heading(&format!("  {}. ", idx + 1), &title);
        let heat_width = display::content_width().saturating_sub(2).max(12);
        for line in html::wrap_line(&heat, heat_width) {
            println!("  {}", display::dim(&line));
        }

        if answers > 0 && !q_id.is_empty() {
            match client.get_question_answers(&q_id, 0, answers, "default") {
                Ok(ans) => {
                    let list = as_array(ans.get("data"));
                    if list.is_empty() {
                        println!("    {}", display::dim("暂无回答"));
                    }
                    for a in list {
                        let author = ptr_str(a, "/author/name");
                        let content = if !json_str(a, "excerpt").is_empty() {
                            html_to_inline(json_str(a, "excerpt"))
                        } else {
                            html_to_inline(json_str(a, "content"))
                        };
                        let upvotes = display::format_count(a.get("voteup_count"));
                        let content = display::truncate(&content, 800);
                        print_hanging(
                            &format!("    {}: ", author),
                            &content,
                            &format!("{} 赞", upvotes),
                        );
                    }
                }
                Err(_) => println!("    {}", display::dim("暂无回答")),
            }
        }
    }
    println!();
    Ok(())
}

pub fn question(question_id: &str, as_json: bool) -> Result<()> {
    let client = client()?;
    let q = client.get_question(question_id).map_err(|e| {
        anyhow::anyhow!(
            "{}（可改用 zhihu answers {} 查看该问题的回答）",
            e,
            question_id
        )
    })?;
    if as_json {
        print_json(&q);
        return Ok(());
    }

    let title = html_to_inline(json_str(&q, "title"));
    let detail = render_body(json_str(&q, "detail"), 2);

    println!();
    print_heading("  ", &title);
    if !detail.trim().is_empty() {
        println!();
        print_body(&detail, 2);
    }
    println!();
    println!(
        "  {}",
        display::stats_line(&[
            ("回答", display::format_count(q.get("answer_count"))),
            ("关注", display::format_count(q.get("follower_count"))),
            ("浏览", display::format_count(q.get("visit_count"))),
        ])
    );
    println!();
    Ok(())
}

pub fn answers(question_id: &str, limit: usize, sort_by: &str, as_json: bool) -> Result<()> {
    let client = client()?;
    let results = client.get_question_answers(question_id, 0, limit, sort_by)?;
    if as_json {
        print_json(&results);
        return Ok(());
    }
    let data = as_array(results.get("data"));
    if data.is_empty() {
        display::info("还没有回答");
        return Ok(());
    }

    let rows = data
        .iter()
        .enumerate()
        .map(|(i, ans)| {
            let author = {
                let name = ptr_str(ans, "/author/name");
                if name.is_empty() {
                    "匿名用户".to_string()
                } else {
                    name.to_string()
                }
            };
            let excerpt = if !json_str(ans, "excerpt").is_empty() {
                html_to_inline(json_str(ans, "excerpt"))
            } else {
                html_to_inline(json_str(ans, "content"))
            };
            vec![
                (i + 1).to_string(),
                author,
                display::truncate(&excerpt, 120),
                display::format_count(ans.get("voteup_count")),
            ]
        })
        .collect();

    display::print_table(
        Some(&format!("回答 — 问题 {}", question_id)),
        &["#", "作者", "摘要", "赞同"],
        rows,
        &[0, 3],
    );
    Ok(())
}

pub fn answer(answer_id: &str, comments: bool, limit: usize, as_json: bool) -> Result<()> {
    let client = client()?;
    let ans = client.get_answer(answer_id)?;
    if as_json {
        print_json(&ans);
        return Ok(());
    }

    let author = {
        let name = ptr_str(&ans, "/author/name");
        if name.is_empty() {
            "匿名用户".to_string()
        } else {
            name.to_string()
        }
    };
    let content = render_body(json_str(&ans, "content"), 2);

    println!();
    println!("  {}", display::title(&format!("{} 的回答", author)));
    println!();
    print_body(&content, 2);
    println!();
    println!(
        "  {}",
        display::stats_line(&[
            ("赞同", display::format_count(ans.get("voteup_count"))),
            ("评论", display::format_count(ans.get("comment_count"))),
        ])
    );
    println!();

    if !comments {
        return Ok(());
    }

    let comment_data: Vec<Value> = if limit == 0 {
        let mut all = Vec::new();
        let mut offset = 0usize;
        loop {
            let result = client.get_answer_comments(answer_id, offset, 20)?;
            let page = as_array(result.get("data")).to_vec();
            all.extend(page.iter().cloned());
            let is_end = result
                .pointer("/paging/is_end")
                .and_then(Value::as_bool)
                .unwrap_or(true);
            if is_end || page.is_empty() {
                break;
            }
            offset += page.len();
        }
        all
    } else {
        client
            .get_answer_comments(answer_id, 0, limit)?
            .get("data")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
    };

    if comment_data.is_empty() {
        display::info("暂无评论");
        return Ok(());
    }
    println!("  {}", display::title("评论"));
    println!();
    for (i, c) in comment_data.iter().enumerate() {
        print_comment(
            i + 1,
            &html_to_inline(json_str(c, "content")),
            &display::format_count(c.get("vote_count")),
        );
    }
    println!();
    Ok(())
}

pub fn feed(limit: usize, as_json: bool) -> Result<()> {
    let client = client()?;
    let results = client.get_feed(limit)?;
    if as_json {
        print_json(&results);
        return Ok(());
    }
    let data = as_array(results.get("data"));
    if data.is_empty() {
        display::info("推荐流为空");
        return Ok(());
    }

    let rows = data
        .iter()
        .map(|item| {
            let target = item.get("target").unwrap_or(item);
            let item_type = json_str(target, "type");
            let item_id = value_to_id(target.get("id"));
            let raw_title = if !json_str(target, "title").is_empty() {
                json_str(target, "title")
            } else if !ptr_str(target, "/question/title").is_empty() {
                ptr_str(target, "/question/title")
            } else {
                json_str(target, "excerpt")
            };
            vec![
                item_id,
                item_type.to_string(),
                html_to_inline(raw_title),
                ptr_str(target, "/author/name").to_string(),
            ]
        })
        .collect();

    display::print_table(
        Some("推荐"),
        &["ID", "类型", "标题 / 摘要", "作者"],
        rows,
        &[],
    );
    Ok(())
}

pub fn feeds(limit: usize, comment_limit: usize, as_json: bool) -> Result<()> {
    let client = client()?;
    let results = client.get_feed(limit)?;
    if as_json {
        print_json(&results);
        return Ok(());
    }
    let data = as_array(results.get("data"));
    if data.is_empty() {
        display::info("推荐流为空");
        return Ok(());
    }

    for (idx, item) in data.iter().enumerate() {
        let target = item.get("target").unwrap_or(item);
        let item_type = json_str(target, "type");
        let item_id = value_to_id(target.get("id"));
        let raw_title = if !json_str(target, "title").is_empty() {
            json_str(target, "title")
        } else if !ptr_str(target, "/question/title").is_empty() {
            ptr_str(target, "/question/title")
        } else {
            json_str(target, "excerpt")
        };
        let title = html_to_inline(raw_title);
        let author = ptr_str(target, "/author/name");

        println!();
        print_heading(&format!("  {}. [{}] ", idx + 1, item_type), &title);
        println!(
            "  {}",
            display::dim(&format!("ID: {}  作者: {}", item_id, author))
        );

        let content_html = if item_type == "answer" && !item_id.is_empty() {
            match client.get_answer(&item_id) {
                Ok(ans) => json_str(&ans, "content").to_string(),
                Err(_) => json_str(target, "excerpt").to_string(),
            }
        } else {
            let content = json_str(target, "content");
            if content.is_empty() {
                json_str(target, "excerpt").to_string()
            } else {
                content.to_string()
            }
        };
        let content = render_body(&content_html, 2);
        if !content.trim().is_empty() {
            println!();
            print_body(&content, 2);
        }

        if comment_limit > 0 && item_type == "answer" {
            let comments = client
                .get_answer_comments(&item_id, 0, comment_limit)
                .ok()
                .and_then(|v| v.get("data").and_then(Value::as_array).cloned())
                .unwrap_or_default();
            if comments.is_empty() {
                println!("    {}", display::dim("暂无评论"));
            } else {
                println!();
                for (i, c) in comments.iter().enumerate() {
                    print_comment(
                        i + 1,
                        &html_to_inline(json_str(c, "content")),
                        &display::format_count(c.get("vote_count")),
                    );
                }
            }
        }
    }
    println!();
    Ok(())
}

pub fn topic(topic_id: &str, as_json: bool) -> Result<()> {
    let client = client()?;
    let t = client.get_topic(topic_id)?;
    if as_json {
        print_json(&t);
        return Ok(());
    }

    let name = json_str(&t, "name");
    let intro = render_body(json_str(&t, "introduction"), 2);

    println!();
    print_heading("  ", &format!("# {}", name));
    if !intro.trim().is_empty() {
        println!();
        print_body(&intro, 2);
    }
    println!();
    println!(
        "  {}",
        display::stats_line(&[
            ("关注", display::format_count(t.get("followers_count"))),
            ("问题", display::format_count(t.get("questions_count"))),
        ])
    );

    if let Ok(hot_q) = client.get_topic_hot_questions(topic_id, 0, 10) {
        let data = as_array(hot_q.get("data"));
        if !data.is_empty() {
            let rows = data
                .iter()
                .enumerate()
                .map(|(i, q)| {
                    vec![
                        (i + 1).to_string(),
                        html_to_inline(json_str(q, "title")),
                        display::format_count(q.get("answer_count")),
                    ]
                })
                .collect();
            display::print_table(Some("热门问题"), &["#", "问题", "回答"], rows, &[0, 2]);
        }
    }
    println!();
    Ok(())
}
