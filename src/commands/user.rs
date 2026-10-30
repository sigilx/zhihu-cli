//! 用户资料类命令。

use anyhow::Result;

use crate::commands::{client, print_json, print_kv};
use crate::display;
use crate::html::html_to_inline;
use crate::util::{as_array, json_i64, json_str};

pub fn user(url_token: &str, as_json: bool) -> Result<()> {
    let client = client()?;
    let info = client.get_user_profile(url_token)?;
    if as_json {
        print_json(&info);
        return Ok(());
    }

    let gender = match json_i64(&info, "gender") {
        0 => "男",
        1 => "女",
        _ => "—",
    };
    let pairs = vec![
        ("Name".to_string(), json_str(&info, "name").to_string()),
        (
            "Headline".to_string(),
            json_str(&info, "headline").to_string(),
        ),
        (
            "Bio".to_string(),
            html_to_inline(json_str(&info, "description")),
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
    ];
    print_kv(&format!("@{}", url_token), &pairs);
    Ok(())
}

pub fn user_answers(url_token: &str, limit: usize, as_json: bool) -> Result<()> {
    let client = client()?;
    let results = client.get_user_answers(url_token, 0, limit, "created")?;
    if as_json {
        print_json(&results);
        return Ok(());
    }
    let data = as_array(results.get("data"));
    if data.is_empty() {
        display::info("没有找到回答");
        return Ok(());
    }

    let rows = data
        .iter()
        .enumerate()
        .map(|(i, ans)| {
            vec![
                (i + 1).to_string(),
                html_to_inline(crate::util::ptr_str(ans, "/question/title")),
                display::format_count(ans.get("voteup_count")),
            ]
        })
        .collect();
    display::print_table(
        Some(&format!("@{} 的回答", url_token)),
        &["#", "问题", "赞同"],
        rows,
        &[0, 2],
    );
    Ok(())
}

pub fn user_articles(url_token: &str, limit: usize, as_json: bool) -> Result<()> {
    let client = client()?;
    let results = client.get_user_articles(url_token, 0, limit, "created")?;
    if as_json {
        print_json(&results);
        return Ok(());
    }
    let data = as_array(results.get("data"));
    if data.is_empty() {
        display::info("没有找到文章");
        return Ok(());
    }

    let rows = data
        .iter()
        .enumerate()
        .map(|(i, art)| {
            vec![
                (i + 1).to_string(),
                html_to_inline(json_str(art, "title")),
                display::format_count(art.get("voteup_count")),
            ]
        })
        .collect();
    display::print_table(
        Some(&format!("@{} 的文章", url_token)),
        &["#", "标题", "赞同"],
        rows,
        &[0, 2],
    );
    Ok(())
}

pub fn followers(url_token: &str, limit: usize, as_json: bool) -> Result<()> {
    let client = client()?;
    let results = client.get_followers(url_token, 0, limit)?;
    if as_json {
        print_json(&results);
        return Ok(());
    }
    print_user_list(results, &format!("@{} 的粉丝", url_token))
}

pub fn following(url_token: &str, limit: usize, as_json: bool) -> Result<()> {
    let client = client()?;
    let results = client.get_following(url_token, 0, limit)?;
    if as_json {
        print_json(&results);
        return Ok(());
    }
    print_user_list(results, &format!("@{} 关注的人", url_token))
}

fn print_user_list(results: serde_json::Value, heading: &str) -> Result<()> {
    let data = as_array(results.get("data"));
    if data.is_empty() {
        display::info("没有找到用户");
        return Ok(());
    }
    let rows = data
        .iter()
        .enumerate()
        .map(|(i, u)| {
            vec![
                (i + 1).to_string(),
                json_str(u, "name").to_string(),
                display::truncate(json_str(u, "headline"), 60),
                display::format_count(u.get("follower_count")),
            ]
        })
        .collect();
    display::print_table(Some(heading), &["#", "用户", "简介", "粉丝"], rows, &[0, 3]);
    Ok(())
}
