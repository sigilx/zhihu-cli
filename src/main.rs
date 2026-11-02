mod auth;
mod client;
mod commands;
mod config;
mod display;
mod html;
mod tui;
mod util;

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use clap::{Parser, Subcommand};

static VERBOSE: AtomicBool = AtomicBool::new(false);

#[derive(Parser)]
#[command(
    name = "zhihu",
    version,
    about = "知乎命令行工具 — 在终端搜索、阅读、互动",
    long_about = "zhihu-cli 的 Rust 实现：搜索、热榜、问题/回答/评论、推荐、用户资料、\n发布提问/想法/文章、收藏与通知。正文 HTML 会按终端宽度重新排版。\n\n不带子命令运行 `zhihu` 可进入交互式两栏阅读器：\n左侧浏览热榜/推荐/搜索，右侧阅读正文，支持后台加载与滚动。"
)]
struct Cli {
    /// 显示详细错误信息
    #[arg(short, long, global = true)]
    verbose: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// 登录（二维码或粘贴 Cookie）
    Login {
        /// 使用二维码登录
        #[arg(long)]
        qrcode: bool,
        /// 直接粘贴 Cookie 字符串（需包含 z_c0、_xsrf、d_c0）
        #[arg(long, value_name = "COOKIE")]
        cookie: Option<String>,
    },
    /// 退出登录，删除本地 cookie
    Logout,
    /// 检查本地登录状态（不联网）
    Status,
    /// 查看当前用户资料
    Whoami {
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 搜索问题 / 回答 / 文章
    Search {
        /// 搜索关键词
        query: String,
        /// 搜索范围：general / people / topic
        #[arg(short = 't', long = "type", default_value = "general", value_parser = ["general", "people", "topic"])]
        search_type: String,
        /// 最大结果数
        /// 最大结果数
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
        /// 每个问题附带几条回答（0=不显示）
        #[arg(short = 'a', long, default_value_t = 3)]
        answers: usize,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 查看知乎热榜
    Hot {
        /// 热榜条数
        #[arg(short, long, default_value_t = 50)]
        limit: usize,
        /// 每条热榜附带几条回答（0=不显示）
        #[arg(short = 'a', long, default_value_t = 3)]
        answers: usize,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 查看问题详情
    Question {
        /// 问题 ID
        question_id: String,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 列出问题的回答
    Answers {
        /// 问题 ID
        question_id: String,
        /// 回答数量
        #[arg(short, long, default_value_t = 5)]
        limit: usize,
        /// 排序：default / created
        #[arg(long, default_value = "default", value_parser = ["default", "created"])]
        sort: String,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 阅读回答正文
    Answer {
        /// 回答 ID
        answer_id: String,
        /// 显示评论
        #[arg(short = 'c', long)]
        comments: bool,
        /// 评论数量（0=全部）
        #[arg(short = 'l', long, default_value_t = 0)]
        limit: usize,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 推荐列表（仅概要）
    Feed {
        /// 最大结果数
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 推荐内容 + 正文 + 评论
    Feeds {
        /// 推荐条数
        #[arg(short, long, default_value_t = 6)]
        limit: usize,
        /// 每条内容附带几条评论（0=不显示）
        #[arg(short = 'c', long = "comment-limit", default_value_t = 10)]
        comment_limit: usize,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 查看话题详情与热门问题
    Topic {
        /// 话题 ID
        topic_id: String,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 查看用户资料
    User {
        /// 用户主页路径中的 url_token
        /// 用户 url_token
        url_token: String,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 查看用户的回答
    UserAnswers {
        /// 用户 url_token
        url_token: String,
        /// 最大结果数
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 查看用户的文章
    UserArticles {
        /// 用户 url_token
        url_token: String,
        /// 最大结果数
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 查看用户的粉丝
    Followers {
        /// 用户 url_token
        url_token: String,
        /// 最大结果数
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 查看用户关注的人
    Following {
        /// 用户 url_token
        url_token: String,
        /// 最大结果数
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 赞同 / 取消赞同回答
    Vote {
        /// 回答 ID
        answer_id: String,
        /// 取消赞同
        #[arg(long)]
        neutral: bool,
    },

    /// 关注 / 取消关注问题
    FollowQuestion {
        /// 问题 ID
        question_id: String,
        /// 取消关注
        #[arg(long)]
        unfollow: bool,
    },

    /// 发布提问（描述支持 HTML）
    Ask {
        /// 标题
        title: String,
        /// 问题描述（支持 HTML）
        #[arg(short, long, default_value = "")]
        detail: String,
        /// 话题 ID（可重复）
        #[arg(short = 't', long = "topic")]
        topics: Vec<String>,
        /// 图片路径（可重复）
        #[arg(short = 'i', long = "image")]
        images: Vec<String>,
    },

    /// 发布想法
    Pin {
        /// 标题
        title: String,
        /// 想法正文（支持 HTML）
        #[arg(short = 'c', long, default_value = "")]
        content: String,
        /// 图片路径（可重复）
        #[arg(short = 'i', long = "image")]
        images: Vec<String>,
    },

    /// 发布文章（正文支持 HTML）
    Article {
        /// 标题
        title: String,
        /// 正文（支持 HTML）
        content: String,
        /// 话题 ID（可重复）
        #[arg(short = 't', long = "topic")]
        topics: Vec<String>,
        /// 图片路径（可重复）
        #[arg(short = 'i', long = "image")]
        images: Vec<String>,
    },

    /// 删除自己的提问
    DeleteQuestion {
        /// 问题 ID
        question_id: String,
        /// 跳过确认
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// 删除自己的想法
    DeletePin {
        /// 想法 ID
        pin_id: String,
        /// 跳过确认
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// 删除自己的文章
    DeleteArticle {
        /// 文章 ID
        article_id: String,
        /// 跳过确认
        #[arg(short = 'y', long)]
        yes: bool,
    },

    /// 查看收藏夹
    Collections {
        /// 最大结果数
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },

    /// 查看通知
    Notifications {
        /// 最大结果数
        #[arg(short, long, default_value_t = 10)]
        limit: usize,
        /// 分页 offset
        #[arg(long, default_value_t = 0)]
        offset: usize,
        /// 输出原始 JSON
        #[arg(long)]
        json: bool,
    },
}

fn run(cli: Cli) -> Result<()> {
    VERBOSE.store(cli.verbose, Ordering::Relaxed);
    let Some(command) = cli.command else {
        return tui::run();
    };
    match command {
        Command::Login { qrcode, cookie } => commands::auth::login(qrcode, cookie),
        Command::Logout => commands::auth::logout(),
        Command::Status => commands::auth::status(),
        Command::Whoami { json } => commands::auth::whoami(json),

        Command::Search {
            query,
            search_type,
            limit,
            answers,
            json,
        } => commands::content::search(&query, &search_type, limit, answers, json),
        Command::Hot {
            limit,
            answers,
            json,
        } => commands::content::hot(limit, answers, json),
        Command::Question { question_id, json } => commands::content::question(&question_id, json),
        Command::Answers {
            question_id,
            limit,
            sort,
            json,
        } => commands::content::answers(&question_id, limit, &sort, json),
        Command::Answer {
            answer_id,
            comments,
            limit,
            json,
        } => commands::content::answer(&answer_id, comments, limit, json),
        Command::Feed { limit, json } => commands::content::feed(limit, json),
        Command::Feeds {
            limit,
            comment_limit,
            json,
        } => commands::content::feeds(limit, comment_limit, json),
        Command::Topic { topic_id, json } => commands::content::topic(&topic_id, json),

        Command::User { url_token, json } => commands::user::user(&url_token, json),
        Command::UserAnswers {
            url_token,
            limit,
            json,
        } => commands::user::user_answers(&url_token, limit, json),
        Command::UserArticles {
            url_token,
            limit,
            json,
        } => commands::user::user_articles(&url_token, limit, json),
        Command::Followers {
            url_token,
            limit,
            json,
        } => commands::user::followers(&url_token, limit, json),
        Command::Following {
            url_token,
            limit,
            json,
        } => commands::user::following(&url_token, limit, json),

        Command::Vote { answer_id, neutral } => commands::interact::vote(&answer_id, neutral),
        Command::FollowQuestion {
            question_id,
            unfollow,
        } => commands::interact::follow_question(&question_id, unfollow),
        Command::Ask {
            title,
            detail,
            topics,
            images,
        } => commands::interact::ask(&title, &detail, &topics, &images),
        Command::Pin {
            title,
            content,
            images,
        } => commands::interact::pin(&title, &content, &images),
        Command::Article {
            title,
            content,
            topics,
            images,
        } => commands::interact::article(&title, &content, &topics, &images),
        Command::DeleteQuestion { question_id, yes } => {
            commands::interact::delete_question(&question_id, yes)
        }
        Command::DeletePin { pin_id, yes } => commands::interact::delete_pin(&pin_id, yes),
        Command::DeleteArticle { article_id, yes } => {
            commands::interact::delete_article(&article_id, yes)
        }
        Command::Collections { limit, json } => commands::interact::collections(limit, json),
        Command::Notifications {
            limit,
            offset,
            json,
        } => commands::interact::notifications(limit, offset, json),
    }
}

fn main() {
    let cli = Cli::parse();
    if let Err(err) = run(cli) {
        let message = if VERBOSE.load(Ordering::Relaxed) {
            format!("{:#}", err)
        } else {
            format!("{}", err)
        };
        display::error(&message);
        std::process::exit(1);
    }
}
