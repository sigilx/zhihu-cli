//! TUI 状态与后台加载。

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::widgets::ListState;
use serde_json::Value;

use crate::auth::{self, cookie_str_to_dict, has_required_cookies, QrLogin};
use crate::client::ZhihuClient;
use crate::util::json_str;

use super::{data, shell};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Hot,
    Feed,
    Search,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Hot, Tab::Feed, Tab::Search];

    pub fn index(self) -> usize {
        match self {
            Tab::Hot => 0,
            Tab::Feed => 1,
            Tab::Search => 2,
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Tab::Hot => "热榜",
            Tab::Feed => "推荐",
            Tab::Search => "搜索",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ItemKind {
    Question,
    Answer,
    Article,
    Pin,
    People,
    Topic,
    Other(String),
}

impl ItemKind {
    pub fn label(&self) -> &'static str {
        match self {
            ItemKind::Question => "问题",
            ItemKind::Answer => "回答",
            ItemKind::Article => "文章",
            ItemKind::Pin => "想法",
            ItemKind::People => "用户",
            ItemKind::Topic => "话题",
            ItemKind::Other(_) => "内容",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ListItem {
    pub id: String,
    pub kind: ItemKind,
    pub title: String,
    pub author: String,
    pub meta: String,
    pub excerpt: String,
    pub raw: Value,
}

impl ListItem {
    pub fn matches(&self, needle: &str) -> bool {
        let haystack = format!(
            "{} {} {} {}",
            self.title, self.author, self.meta, self.excerpt
        )
        .to_lowercase();
        haystack.contains(needle)
    }

    /// 生成可在浏览器打开的网页地址。
    pub fn url(&self) -> String {
        let raw_url = json_str(&self.raw, "url");
        if raw_url.starts_with("https://www.zhihu.com/")
            || raw_url.starts_with("https://zhuanlan.zhihu.com/")
        {
            return raw_url.to_string();
        }
        match &self.kind {
            ItemKind::Question => format!("https://www.zhihu.com/question/{}", self.id),
            ItemKind::Answer => format!("https://www.zhihu.com/answer/{}", self.id),
            ItemKind::Article => format!("https://zhuanlan.zhihu.com/p/{}", self.id),
            ItemKind::Pin => format!("https://www.zhihu.com/pin/{}", self.id),
            ItemKind::People => {
                let token = {
                    let token = json_str(&self.raw, "url_token");
                    if token.is_empty() {
                        self.id.clone()
                    } else {
                        token.to_string()
                    }
                };
                format!("https://www.zhihu.com/people/{token}")
            }
            ItemKind::Topic => format!("https://www.zhihu.com/topic/{}", self.id),
            ItemKind::Other(_) => raw_url.to_string(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Detail {
    Question {
        question: Value,
        answers: Vec<Value>,
    },
    Answer {
        answer: Value,
        comments: Vec<Value>,
    },
    Raw {
        title: String,
        author: String,
        meta: String,
        html: String,
        kind: ItemKind,
    },
    Profile {
        user: Value,
    },
    Topic {
        topic: Value,
    },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Focus {
    List,
    Detail,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LineKind {
    Title,
    Heading,
    Meta,
    Body,
    Comment,
    Blank,
}

#[derive(Clone, Debug)]
pub struct StyledLine {
    pub text: String,
    pub kind: LineKind,
}

#[derive(Clone, Debug)]
pub struct DetailCache {
    pub width: u16,
    pub generation: u64,
    pub lines: Vec<StyledLine>,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub text: String,
    pub error: bool,
}

impl Status {
    pub fn info(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: false,
        }
    }

    pub fn error(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            error: true,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputKind {
    Search,
    Filter,
}

#[derive(Clone, Debug)]
pub struct Input {
    pub kind: InputKind,
    pub buffer: String,
    /// 光标位置，按字符计数。
    pub cursor: usize,
}

impl Input {
    fn new(kind: InputKind, buffer: String) -> Self {
        let cursor = buffer.chars().count();
        Self {
            kind,
            buffer,
            cursor,
        }
    }

    pub fn split(&self) -> (&str, &str) {
        let index = byte_index(&self.buffer, self.cursor);
        (&self.buffer[..index], &self.buffer[index..])
    }
}

fn byte_index(text: &str, char_index: usize) -> usize {
    text.char_indices()
        .nth(char_index)
        .map(|(index, _)| index)
        .unwrap_or(text.len())
}

fn insert_char(text: &mut String, char_index: usize, ch: char) {
    let index = byte_index(text, char_index);
    text.insert(index, ch);
}

fn remove_char(text: &mut String, char_index: usize) -> bool {
    let start = byte_index(text, char_index);
    let end = byte_index(text, char_index + 1);
    if start < end {
        text.replace_range(start..end, "");
        true
    } else {
        false
    }
}

fn remove_previous_word(text: &mut String, char_index: usize) -> usize {
    let mut index = char_index;
    let chars: Vec<char> = text.chars().collect();
    while index > 0 && chars[index - 1].is_whitespace() {
        index -= 1;
    }
    while index > 0 && !chars[index - 1].is_whitespace() {
        index -= 1;
    }
    let start = byte_index(text, index);
    let end = byte_index(text, char_index);
    text.replace_range(start..end, "");
    index
}

#[derive(Clone, Debug)]
pub enum LoginView {
    Menu {
        message: Option<String>,
    },
    Qr {
        lines: Vec<String>,
        link: String,
        status: String,
        error: Option<String>,
        cancel: Arc<AtomicBool>,
    },
    Cookie {
        buffer: String,
        cursor: usize,
        error: Option<String>,
        verifying: bool,
    },
}

enum Job {
    SetClient(Option<Arc<ZhihuClient>>),
    List {
        seq: u64,
        tab: Tab,
        query: String,
    },
    Detail {
        seq: u64,
        item: ListItem,
    },
    VerifyCookie {
        seq: u64,
        cookies: HashMap<String, String>,
    },
    Whoami,
}

enum Reply {
    List {
        seq: u64,
        tab: Tab,
        result: Result<Vec<ListItem>, String>,
    },
    Detail {
        seq: u64,
        result: Result<Detail, String>,
    },
    LoginQrReady {
        seq: u64,
        lines: Vec<String>,
        link: String,
    },
    LoginQrDone {
        seq: u64,
        result: Result<HashMap<String, String>, String>,
    },
    LoginCookie {
        seq: u64,
        result: Result<HashMap<String, String>, String>,
    },
    Whoami {
        name: Option<String>,
        auth_error: Option<String>,
    },
}

pub struct App {
    pub tab: Tab,
    pub lists: [Vec<ListItem>; 3],
    pub all_lists: [Vec<ListItem>; 3],
    pub filters: [String; 3],
    pub selected: [usize; 3],
    pub list_loaded: [bool; 3],
    pub list_loading: [bool; 3],
    list_seq: [u64; 3],
    pub detail: Option<Detail>,
    pub detail_loading: bool,
    detail_seq: u64,
    detail_key: Option<String>,
    pub detail_gen: u64,
    pub detail_cache: Option<DetailCache>,
    pub detail_scroll: u16,
    /// 正文窗格可见行数，由 UI 每帧更新，用于半页滚动。
    pub detail_view_height: u16,
    pub focus: Focus,
    pub input: Option<Input>,
    pub search_query: String,
    pub status: Option<Status>,
    pub spinner: usize,
    pub dirty: bool,
    pub list_state: ListState,
    pub login: Option<LoginView>,
    login_seq: u64,
    pub help: bool,
    pub confirm_logout: bool,
    pub user: Option<String>,
    tx: Sender<Job>,
    reply_tx: Sender<Reply>,
    rx: Receiver<Reply>,
    next_seq: u64,
}

fn worker(job_rx: Receiver<Job>, tx: Sender<Reply>) {
    let mut client: Option<Arc<ZhihuClient>> = None;
    while let Ok(job) = job_rx.recv() {
        match job {
            Job::SetClient(new_client) => client = new_client,
            Job::List { seq, tab, query } => {
                let result = match client.as_ref() {
                    Some(client) => {
                        data::load_list(client, tab, &query).map_err(|err| err.to_string())
                    }
                    None => Err("未登录".to_string()),
                };
                if tx.send(Reply::List { seq, tab, result }).is_err() {
                    break;
                }
            }
            Job::Detail { seq, item } => {
                let result = match client.as_ref() {
                    Some(client) => data::load_detail(client, &item).map_err(|err| err.to_string()),
                    None => Err("未登录".to_string()),
                };
                if tx.send(Reply::Detail { seq, result }).is_err() {
                    break;
                }
            }
            Job::VerifyCookie { seq, cookies } => {
                let result = verify_cookie_login(cookies).map_err(|err| err.to_string());
                if tx.send(Reply::LoginCookie { seq, result }).is_err() {
                    break;
                }
            }
            Job::Whoami => {
                let (name, auth_error) = match client.as_ref() {
                    Some(client) => match client.get_self_info() {
                        Ok(info) => {
                            let name = json_str(&info, "name");
                            let name = if name.is_empty() {
                                None
                            } else {
                                Some(name.to_string())
                            };
                            (name, None)
                        }
                        Err(err) => {
                            let message = err.to_string();
                            if message.contains("401")
                                || message.contains("403")
                                || message.contains("登录态已失效")
                                || message.contains("未登录")
                            {
                                (None, Some(message))
                            } else {
                                (None, None)
                            }
                        }
                    },
                    None => (None, None),
                };
                if tx.send(Reply::Whoami { name, auth_error }).is_err() {
                    break;
                }
            }
        }
    }
}

fn verify_cookie_login(cookies: HashMap<String, String>) -> Result<HashMap<String, String>> {
    if !has_required_cookies(&cookies) {
        bail!("Cookie 必须包含 z_c0、_xsrf、d_c0");
    }
    let client = ZhihuClient::new(&cookies)?;
    let info = client
        .get_self_info()
        .map_err(|err| anyhow::anyhow!("验证 Cookie 失败：{err:#}"))?;
    if info.as_object().is_none_or(|obj| obj.is_empty()) {
        bail!("Cookie 无效或已过期");
    }
    auth::save_cookies(&cookies)?;
    Ok(cookies)
}

impl App {
    pub fn new() -> Self {
        let (tx, rx) = mpsc::channel::<Reply>();
        let (job_tx, job_rx) = mpsc::channel::<Job>();
        let reply_tx = tx.clone();
        thread::Builder::new()
            .name("zhihu-tui-loader".into())
            .spawn(move || worker(job_rx, tx))
            .expect("启动 TUI 后台线程失败");

        let mut app = Self {
            tab: Tab::Hot,
            lists: std::array::from_fn(|_| Vec::new()),
            all_lists: std::array::from_fn(|_| Vec::new()),
            filters: std::array::from_fn(|_| String::new()),
            selected: [0; 3],
            list_loaded: [false; 3],
            list_loading: [false; 3],
            list_seq: [0; 3],
            detail: None,
            detail_loading: false,
            detail_seq: 0,
            detail_key: None,
            detail_gen: 0,
            detail_cache: None,
            detail_scroll: 0,
            detail_view_height: 12,
            focus: Focus::List,
            input: None,
            search_query: String::new(),
            status: None,
            spinner: 0,
            dirty: true,
            list_state: ListState::default(),
            login: None,
            login_seq: 0,
            help: false,
            confirm_logout: false,
            user: None,
            tx: job_tx,
            reply_tx,
            rx,
            next_seq: 0,
        };

        match crate::commands::client() {
            Ok(client) => {
                let _ = app.tx.send(Job::SetClient(Some(Arc::new(client))));
                app.whoami();
                app.load_list(Tab::Hot);
            }
            Err(err) => {
                app.login = Some(LoginView::Menu {
                    message: Some(friendly_auth_message(&err.to_string())),
                });
            }
        }
        app
    }

    fn next_seq(&mut self) -> u64 {
        self.next_seq += 1;
        self.next_seq
    }

    fn load_list(&mut self, tab: Tab) {
        if tab == Tab::Search && self.search_query.trim().is_empty() {
            self.list_loading[tab.index()] = false;
            self.input = Some(Input::new(InputKind::Search, String::new()));
            self.status = Some(Status::info("输入关键词后回车搜索"));
            self.dirty = true;
            return;
        }
        let seq = self.next_seq();
        let idx = tab.index();
        self.list_seq[idx] = seq;
        self.list_loading[idx] = true;
        self.list_loaded[idx] = false;
        let query = if tab == Tab::Search {
            self.search_query.clone()
        } else {
            String::new()
        };
        let _ = self.tx.send(Job::List { seq, tab, query });
        self.dirty = true;
    }

    pub fn refresh_tab(&mut self, tab: Tab) {
        if tab == Tab::Search && self.search_query.trim().is_empty() {
            self.input = Some(Input::new(InputKind::Search, String::new()));
            self.status = Some(Status::info("输入关键词后回车搜索"));
            self.dirty = true;
            return;
        }
        // 列表刷新后条目顺序可能变化，先丢掉旧的预览/正文，避免左右错位。
        if self.focus == Focus::List {
            self.clear_detail();
        }
        self.status = None;
        self.load_list(tab);
    }

    fn start_search(&mut self, query: String) {
        self.search_query = query;
        self.tab = Tab::Search;
        self.selected[Tab::Search.index()] = 0;
        self.lists[Tab::Search.index()].clear();
        self.all_lists[Tab::Search.index()].clear();
        self.filters[Tab::Search.index()].clear();
        self.clear_detail();
        self.load_list(Tab::Search);
    }

    fn apply_filter(&mut self, tab: Tab) {
        let idx = tab.index();
        let needle = self.filters[idx].trim().to_lowercase();
        let items: Vec<ListItem> = if needle.is_empty() {
            self.all_lists[idx].clone()
        } else {
            self.all_lists[idx]
                .iter()
                .filter(|item| item.matches(&needle))
                .cloned()
                .collect()
        };
        self.selected[idx] = self.selected[idx].min(items.len().saturating_sub(1));
        self.lists[idx] = items;
    }

    fn set_filter(&mut self, tab: Tab, value: String) {
        self.filters[tab.index()] = value;
        self.apply_filter(tab);
        if self.tab == tab && self.focus == Focus::List && self.detail.is_some() {
            self.clear_detail();
        }
        self.dirty = true;
    }

    fn clear_filter(&mut self, tab: Tab) {
        if self.filters[tab.index()].is_empty() {
            return;
        }
        self.filters[tab.index()].clear();
        self.apply_filter(tab);
        if self.detail.is_some() {
            self.clear_detail();
        }
        self.status = Some(Status::info("已清除筛选"));
        self.dirty = true;
    }

    pub fn active_filter(&self) -> &str {
        &self.filters[self.tab.index()]
    }

    pub fn total_count(&self) -> usize {
        self.all_lists[self.tab.index()].len()
    }

    pub fn current_item(&self) -> Option<&ListItem> {
        self.lists[self.tab.index()].get(self.selected[self.tab.index()])
    }

    pub fn current_list(&self) -> &[ListItem] {
        &self.lists[self.tab.index()]
    }

    pub fn selected_index(&self) -> usize {
        self.selected[self.tab.index()]
    }

    pub fn is_loading(&self) -> bool {
        self.detail_loading
            || self.list_loading[self.tab.index()]
            || matches!(
                self.login,
                Some(LoginView::Cookie {
                    verifying: true,
                    ..
                }) | Some(LoginView::Qr { .. })
            )
    }

    pub fn spinner_text(&self) -> &'static str {
        const FRAMES: [&str; 8] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧"];
        FRAMES[self.spinner % FRAMES.len()]
    }

    pub fn tick(&mut self) {
        self.spinner = self.spinner.wrapping_add(1);
        if self.is_loading() {
            self.dirty = true;
        }
    }

    pub fn poll(&mut self) {
        loop {
            match self.rx.try_recv() {
                Ok(Reply::List { seq, tab, result }) => {
                    let idx = tab.index();
                    if seq != self.list_seq[idx] {
                        continue;
                    }
                    self.list_loading[idx] = false;
                    match result {
                        Ok(items) => {
                            self.list_loaded[idx] = true;
                            self.all_lists[idx] = items;
                            self.apply_filter(tab);
                            if tab == self.tab && self.login.is_none() {
                                if self.all_lists[idx].is_empty() {
                                    self.status =
                                        Some(Status::info("没有内容，按 / 搜索或 r 刷新"));
                                } else if self.lists[idx].is_empty() {
                                    self.status =
                                        Some(Status::info("没有匹配的内容，按 Esc 清除筛选"));
                                } else {
                                    self.status = None;
                                }
                            }
                        }
                        Err(err) => {
                            if tab == self.tab {
                                if self.login.is_none() {
                                    self.status = Some(Status::error(&err));
                                }
                                if is_auth_error(&err) {
                                    self.show_login(&err);
                                }
                            }
                        }
                    }
                    self.dirty = true;
                }
                Ok(Reply::Detail { seq, result }) => {
                    if seq != self.detail_seq {
                        continue;
                    }
                    self.detail_loading = false;
                    match result {
                        Ok(detail) => {
                            self.detail = Some(detail);
                            self.detail_gen += 1;
                            self.detail_cache = None;
                            self.detail_scroll = 0;
                            self.focus = Focus::Detail;
                            self.status = None;
                        }
                        Err(err) => {
                            self.clear_detail();
                            if self.login.is_none() {
                                self.status = Some(Status::error(&err));
                            }
                            if is_auth_error(&err) {
                                self.show_login(&err);
                            }
                        }
                    }
                    self.dirty = true;
                }
                Ok(Reply::LoginQrReady { seq, lines, link }) => {
                    if seq != self.login_seq {
                        continue;
                    }
                    if let Some(LoginView::Qr {
                        lines: current_lines,
                        link: current_link,
                        status,
                        error,
                        ..
                    }) = &mut self.login
                    {
                        *current_lines = lines;
                        *current_link = link;
                        *status = "等待扫码 / 手机确认…".to_string();
                        *error = None;
                    }
                    self.dirty = true;
                }
                Ok(Reply::LoginQrDone { seq, result }) => {
                    if seq != self.login_seq {
                        continue;
                    }
                    match result {
                        Ok(cookies) => {
                            if let Err(err) = self.finish_login(cookies) {
                                self.login = Some(LoginView::Menu {
                                    message: Some(format!("登录成功但初始化失败：{err}")),
                                });
                            }
                        }
                        Err(err) => {
                            if let Some(LoginView::Qr { status, error, .. }) = &mut self.login {
                                *status = "二维码登录未完成".to_string();
                                *error = Some(err);
                            }
                        }
                    }
                    self.dirty = true;
                }
                Ok(Reply::LoginCookie { seq, result }) => {
                    if seq != self.login_seq {
                        continue;
                    }
                    match result {
                        Ok(cookies) => {
                            if let Err(err) = self.finish_login(cookies) {
                                self.login = Some(LoginView::Menu {
                                    message: Some(format!("登录成功但初始化失败：{err}")),
                                });
                            }
                        }
                        Err(err) => {
                            if let Some(LoginView::Cookie {
                                error, verifying, ..
                            }) = &mut self.login
                            {
                                *verifying = false;
                                *error = Some(err);
                            }
                        }
                    }
                    self.dirty = true;
                }
                Ok(Reply::Whoami { name, auth_error }) => {
                    if let Some(name) = name {
                        self.user = Some(name);
                    }
                    if let Some(err) = auth_error {
                        self.show_login(&err);
                    }
                    self.dirty = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.status = Some(Status::error("后台加载线程已退出"));
                    self.dirty = true;
                    break;
                }
            }
        }
    }

    fn finish_login(&mut self, cookies: HashMap<String, String>) -> Result<()> {
        if !has_required_cookies(&cookies) {
            bail!("登录 Cookie 不完整（缺少 z_c0/_xsrf/d_c0）");
        }
        auth::save_cookies(&cookies)?;
        let client = Arc::new(ZhihuClient::new(&cookies)?);
        let _ = self.tx.send(Job::SetClient(Some(client)));
        self.login = None;
        self.login_seq = self.next_seq();
        self.reset_lists();
        self.status = Some(Status::info("登录成功"));
        self.load_list(Tab::Hot);
        self.whoami();
        Ok(())
    }

    fn whoami(&mut self) {
        let _ = self.tx.send(Job::Whoami);
    }

    fn reset_lists(&mut self) {
        for idx in 0..3 {
            let seq = self.next_seq();
            self.list_seq[idx] = seq;
            self.lists[idx].clear();
            self.all_lists[idx].clear();
            self.filters[idx].clear();
            self.selected[idx] = 0;
            self.list_loaded[idx] = false;
            self.list_loading[idx] = false;
        }
        self.clear_detail();
        self.tab = Tab::Hot;
        self.focus = Focus::List;
        self.search_query.clear();
        self.input = None;
        self.help = false;
        self.confirm_logout = false;
        self.user = None;
        self.status = None;
        self.dirty = true;
    }

    fn show_login(&mut self, message: &str) {
        if self.login.is_some() {
            return;
        }
        self.login = Some(LoginView::Menu {
            message: Some(friendly_auth_message(message)),
        });
        self.status = None;
        self.input = None;
        self.help = false;
        self.confirm_logout = false;
        self.dirty = true;
    }

    fn logout(&mut self) {
        let removed = crate::auth::clear_cookies();
        let _ = self.tx.send(Job::SetClient(None));
        self.reset_lists();
        let message = match removed {
            Ok(removed) if !removed.is_empty() => {
                format!("已退出登录（删除 {}）", removed.join(", "))
            }
            Ok(_) => "已退出登录".to_string(),
            Err(err) => format!("已退出登录，但删除 cookie 失败：{err}"),
        };
        self.login = Some(LoginView::Menu {
            message: Some(message),
        });
        self.dirty = true;
    }

    fn start_qr_login(&mut self) {
        let seq = self.next_seq();
        self.login_seq = seq;
        let cancel = Arc::new(AtomicBool::new(false));
        self.login = Some(LoginView::Qr {
            lines: Vec::new(),
            link: String::new(),
            status: "正在获取二维码…".to_string(),
            error: None,
            cancel: cancel.clone(),
        });

        let tx = self.reply_tx.clone();
        let _ = thread::Builder::new()
            .name("zhihu-tui-login".into())
            .spawn(move || {
                let result = (|| -> std::result::Result<HashMap<String, String>, String> {
                    let mut session = QrLogin::start().map_err(|err| err.to_string())?;
                    let lines = auth::qr_lines(&session.link).map_err(|err| err.to_string())?;
                    if tx
                        .send(Reply::LoginQrReady {
                            seq,
                            lines,
                            link: session.link.clone(),
                        })
                        .is_err()
                    {
                        return Err("界面已关闭".to_string());
                    }
                    while !session.is_expired() {
                        if cancel.load(Ordering::Relaxed) {
                            return Err("已取消".to_string());
                        }
                        match session.poll() {
                            Ok(Some(cookies)) => return Ok(cookies),
                            Ok(None) => {}
                            Err(err) => return Err(err.to_string()),
                        }
                        thread::sleep(Duration::from_millis(300));
                    }
                    session.finish().map_err(|err| err.to_string())
                })();
                let _ = tx.send(Reply::LoginQrDone { seq, result });
            });
    }

    fn cancel_qr(&mut self) {
        if let Some(LoginView::Qr { cancel, .. }) = &self.login {
            cancel.store(true, Ordering::Relaxed);
        }
        self.login_seq = self.next_seq();
        self.login = Some(LoginView::Menu { message: None });
        self.dirty = true;
    }

    /// 登录界面的按键处理；返回 true 表示退出 TUI。
    fn handle_login_key(&mut self, key: KeyEvent) -> bool {
        if self.login.is_none() {
            return false;
        }

        if matches!(self.login, Some(LoginView::Menu { .. })) {
            match key.code {
                KeyCode::Char('1') | KeyCode::Enter => self.start_qr_login(),
                KeyCode::Char('2') | KeyCode::Char('c') => {
                    self.login = Some(LoginView::Cookie {
                        buffer: String::new(),
                        cursor: 0,
                        error: None,
                        verifying: false,
                    });
                    self.dirty = true;
                }
                KeyCode::Char('q') | KeyCode::Esc => return true,
                _ => {}
            }
            return false;
        }

        if matches!(self.login, Some(LoginView::Qr { .. })) {
            let has_error = matches!(&self.login, Some(LoginView::Qr { error: Some(_), .. }));
            match key.code {
                KeyCode::Esc => self.cancel_qr(),
                KeyCode::Char('q') => return true,
                KeyCode::Char('r') if has_error => self.start_qr_login(),
                _ => {}
            }
            return false;
        }

        let mut cancel = false;
        let mut submit = false;
        if let Some(LoginView::Cookie {
            buffer,
            cursor,
            verifying,
            ..
        }) = self.login.as_mut()
        {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                if matches!(key.code, KeyCode::Char('u')) {
                    buffer.clear();
                    *cursor = 0;
                    self.dirty = true;
                }
            } else {
                match key.code {
                    KeyCode::Esc => cancel = true,
                    KeyCode::Enter if !*verifying => submit = true,
                    KeyCode::Left => {
                        *cursor = cursor.saturating_sub(1);
                        self.dirty = true;
                    }
                    KeyCode::Right => {
                        *cursor = (*cursor + 1).min(buffer.chars().count());
                        self.dirty = true;
                    }
                    KeyCode::Home => {
                        *cursor = 0;
                        self.dirty = true;
                    }
                    KeyCode::End => {
                        *cursor = buffer.chars().count();
                        self.dirty = true;
                    }
                    KeyCode::Backspace => {
                        if *cursor > 0 {
                            remove_char(buffer, *cursor - 1);
                            *cursor -= 1;
                            self.dirty = true;
                        }
                    }
                    KeyCode::Delete => {
                        if remove_char(buffer, *cursor) {
                            self.dirty = true;
                        }
                    }
                    KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::ALT) => {
                        insert_char(buffer, *cursor, ch);
                        *cursor += 1;
                        self.dirty = true;
                    }
                    _ => {}
                }
            }
        }

        if cancel {
            self.login_seq = self.next_seq();
            self.login = Some(LoginView::Menu { message: None });
            self.dirty = true;
            return false;
        }

        if submit {
            let parsed = match &self.login {
                Some(LoginView::Cookie { buffer, .. }) => cookie_str_to_dict(buffer),
                _ => return false,
            };
            if !has_required_cookies(&parsed) {
                if let Some(LoginView::Cookie { error, .. }) = &mut self.login {
                    *error = Some("Cookie 必须包含 z_c0、_xsrf、d_c0".to_string());
                }
                self.dirty = true;
                return false;
            }
            if let Some(LoginView::Cookie {
                error, verifying, ..
            }) = &mut self.login
            {
                *verifying = true;
                *error = None;
            }
            let seq = self.next_seq();
            self.login_seq = seq;
            let _ = self.tx.send(Job::VerifyCookie {
                seq,
                cookies: parsed,
            });
            self.dirty = true;
        }
        false
    }

    fn go_back(&mut self) {
        if self.focus == Focus::Detail {
            self.focus = Focus::List;
        } else if self.detail.is_some() {
            self.clear_detail();
        } else if !self.filters[self.tab.index()].is_empty() {
            self.clear_filter(self.tab);
        }
        self.dirty = true;
    }

    fn copy_current(&mut self) {
        let url = self.current_item().map(ListItem::url).unwrap_or_default();
        if url.is_empty() {
            self.status = Some(Status::error("当前条目没有可复制的链接"));
        } else {
            match shell::copy_text(&url) {
                Ok(()) => self.status = Some(Status::info("已复制链接")),
                Err(err) => self.status = Some(Status::error(format!("复制失败：{err}"))),
            }
        }
        self.dirty = true;
    }

    fn open_current(&mut self) {
        let url = self.current_item().map(ListItem::url).unwrap_or_default();
        if url.is_empty() {
            self.status = Some(Status::error("当前条目没有可打开的链接"));
        } else {
            match shell::open_url(&url) {
                Ok(()) => self.status = Some(Status::info("已在浏览器打开")),
                Err(err) => self.status = Some(Status::error(format!("打开失败：{err}"))),
            }
        }
        self.dirty = true;
    }

    fn clear_detail(&mut self) {
        self.detail = None;
        self.detail_key = None;
        self.detail_loading = false;
        self.detail_seq = self.next_seq();
        self.detail_gen += 1;
        self.detail_cache = None;
        self.detail_scroll = 0;
        self.dirty = true;
    }

    fn open_selected(&mut self) {
        let Some(item) = self.current_item().cloned() else {
            return;
        };
        let key = format!("{}:{}", item.kind.label(), item.id);
        if self.detail.is_some() && self.detail_key.as_deref() == Some(key.as_str()) {
            self.focus = Focus::Detail;
            self.dirty = true;
            return;
        }
        let seq = self.next_seq();
        self.detail_seq = seq;
        self.detail_loading = true;
        self.detail_key = Some(key);
        self.detail = None;
        self.detail_cache = None;
        self.detail_gen += 1;
        self.detail_scroll = 0;
        self.focus = Focus::List;
        self.status = Some(Status::info("正在读取…"));
        let _ = self.tx.send(Job::Detail { seq, item });
        self.dirty = true;
    }

    fn switch_tab(&mut self, tab: Tab) {
        if tab == self.tab && self.detail.is_none() {
            return;
        }
        self.tab = tab;
        self.clear_detail();
        self.focus = Focus::List;
        self.status = None;
        if tab == Tab::Search && self.search_query.trim().is_empty() {
            self.input = Some(Input::new(InputKind::Search, String::new()));
            self.status = Some(Status::info("输入关键词后回车搜索"));
            self.dirty = true;
            return;
        }
        let idx = tab.index();
        if !self.list_loaded[idx] && !self.list_loading[idx] {
            self.load_list(tab);
        }
        self.dirty = true;
    }

    fn move_selection(&mut self, delta: isize) {
        let idx = self.tab.index();
        let len = self.lists[idx].len();
        if len == 0 {
            return;
        }
        if self.focus == Focus::List && self.detail.is_some() {
            self.clear_detail();
        }
        let current = self.selected[idx] as isize + delta;
        self.selected[idx] = current.clamp(0, len as isize - 1) as usize;
        self.dirty = true;
    }

    fn select_first(&mut self) {
        if self.focus == Focus::List && self.detail.is_some() {
            self.clear_detail();
        }
        self.selected[self.tab.index()] = 0;
        self.dirty = true;
    }

    fn select_last(&mut self) {
        if self.focus == Focus::List && self.detail.is_some() {
            self.clear_detail();
        }
        let idx = self.tab.index();
        self.selected[idx] = self.lists[idx].len().saturating_sub(1);
        self.dirty = true;
    }

    fn scroll_detail(&mut self, delta: isize) {
        let next = self.detail_scroll as isize + delta;
        self.detail_scroll = next.clamp(0, u16::MAX as isize) as u16;
        self.dirty = true;
    }

    fn half_page(&self) -> isize {
        (self.detail_view_height as isize / 2).max(1)
    }

    fn full_page(&self) -> isize {
        (self.detail_view_height as isize - 1).max(1)
    }

    fn handle_input_key(&mut self, key: KeyEvent) {
        let Some(kind) = self.input.as_ref().map(|input| input.kind) else {
            return;
        };
        let mut changed = false;
        let mut submit = false;
        let mut cancel = false;

        if let Some(input) = self.input.as_mut() {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                match key.code {
                    KeyCode::Char('u') => {
                        input.buffer.clear();
                        input.cursor = 0;
                        changed = true;
                    }
                    KeyCode::Char('w') => {
                        input.cursor = remove_previous_word(&mut input.buffer, input.cursor);
                        changed = true;
                    }
                    KeyCode::Char('a') => input.cursor = 0,
                    KeyCode::Char('e') => input.cursor = input.buffer.chars().count(),
                    _ => {}
                }
            } else {
                match key.code {
                    KeyCode::Esc => cancel = true,
                    KeyCode::Enter => submit = true,
                    KeyCode::Left => {
                        input.cursor = input.cursor.saturating_sub(1);
                    }
                    KeyCode::Right => {
                        input.cursor = (input.cursor + 1).min(input.buffer.chars().count());
                    }
                    KeyCode::Home => input.cursor = 0,
                    KeyCode::End => input.cursor = input.buffer.chars().count(),
                    KeyCode::Backspace => {
                        if input.cursor > 0 {
                            remove_char(&mut input.buffer, input.cursor - 1);
                            input.cursor -= 1;
                            changed = true;
                        }
                    }
                    KeyCode::Delete => {
                        if remove_char(&mut input.buffer, input.cursor) {
                            changed = true;
                        }
                    }
                    KeyCode::Char(ch) if !key.modifiers.contains(KeyModifiers::ALT) => {
                        insert_char(&mut input.buffer, input.cursor, ch);
                        input.cursor += 1;
                        changed = true;
                    }
                    _ => {}
                }
            }
        }

        if changed && kind == InputKind::Filter {
            if let Some(input) = self.input.as_ref() {
                let value = input.buffer.clone();
                self.set_filter(self.tab, value);
            }
        }

        if cancel {
            self.input = None;
            if kind == InputKind::Filter {
                self.clear_filter(self.tab);
            } else {
                self.dirty = true;
            }
            return;
        }

        if submit {
            let value = self
                .input
                .as_ref()
                .map(|input| input.buffer.trim().to_string())
                .unwrap_or_default();
            self.input = None;
            match kind {
                InputKind::Search => {
                    if value.is_empty() {
                        self.status = Some(Status::info("关键词不能为空"));
                        self.dirty = true;
                    } else {
                        self.start_search(value);
                    }
                }
                InputKind::Filter => {
                    self.set_filter(self.tab, value);
                    let count = self.current_list().len();
                    self.status = Some(Status::info(format!("筛选：{count} 条，按 Esc 清除")));
                    self.dirty = true;
                }
            }
        }
    }

    /// 返回 true 表示退出 TUI。
    pub fn handle_key(&mut self, key: KeyEvent) -> bool {
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            return true;
        }
        if self.login.is_some() {
            return self.handle_login_key(key);
        }
        if self.help {
            if matches!(
                key.code,
                KeyCode::Esc | KeyCode::Char('?') | KeyCode::Char('q')
            ) {
                self.help = false;
                self.dirty = true;
            }
            return false;
        }
        if self.confirm_logout {
            match key.code {
                KeyCode::Char('y') | KeyCode::Char('Y') => {
                    self.confirm_logout = false;
                    self.logout();
                }
                KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => {
                    self.confirm_logout = false;
                    self.dirty = true;
                }
                _ => {}
            }
            return false;
        }
        if self.input.is_some() {
            self.handle_input_key(key);
            return false;
        }

        if key.modifiers.contains(KeyModifiers::CONTROL) {
            match key.code {
                KeyCode::Char('d') => {
                    self.scroll_detail(self.half_page());
                    return false;
                }
                KeyCode::Char('u') => {
                    self.scroll_detail(-self.half_page());
                    return false;
                }
                KeyCode::Char('f') => {
                    self.scroll_detail(self.full_page());
                    return false;
                }
                KeyCode::Char('b') => {
                    self.scroll_detail(-self.full_page());
                    return false;
                }
                _ => {}
            }
        }

        match key.code {
            KeyCode::Char('q') => return true,
            KeyCode::Char('?') => {
                self.help = true;
                self.dirty = true;
            }
            KeyCode::Char('1') => self.switch_tab(Tab::Hot),
            KeyCode::Char('2') => self.switch_tab(Tab::Feed),
            KeyCode::Char('3') => self.switch_tab(Tab::Search),
            KeyCode::Tab => {
                let next = (self.tab.index() + 1) % Tab::ALL.len();
                self.switch_tab(Tab::ALL[next]);
            }
            KeyCode::Char('/') => {
                self.input = Some(Input::new(InputKind::Search, self.search_query.clone()));
                self.dirty = true;
            }
            KeyCode::Char('f') if self.focus == Focus::List => {
                self.input = Some(Input::new(
                    InputKind::Filter,
                    self.filters[self.tab.index()].clone(),
                ));
                self.dirty = true;
            }
            KeyCode::Char('r') => {
                self.refresh_tab(self.tab);
            }
            KeyCode::Char('y') => self.copy_current(),
            KeyCode::Char('o') => self.open_current(),
            KeyCode::Char('L') => {
                self.confirm_logout = true;
                self.dirty = true;
            }
            KeyCode::Esc | KeyCode::Left => self.go_back(),
            KeyCode::Char('h') if !key.modifiers.contains(KeyModifiers::CONTROL) => self.go_back(),
            _ => match self.focus {
                Focus::List => match key.code {
                    KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
                    KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
                    KeyCode::PageDown | KeyCode::Char(' ') => self.move_selection(10),
                    KeyCode::PageUp => self.move_selection(-10),
                    KeyCode::Home | KeyCode::Char('g') => self.select_first(),
                    KeyCode::End | KeyCode::Char('G') => self.select_last(),
                    KeyCode::Enter | KeyCode::Right => self.open_selected(),
                    KeyCode::Char('l') if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        self.open_selected()
                    }
                    _ => {}
                },
                Focus::Detail => match key.code {
                    KeyCode::Down => self.scroll_detail(self.half_page()),
                    KeyCode::Up => self.scroll_detail(-self.half_page()),
                    KeyCode::Char('j') => self.scroll_detail(1),
                    KeyCode::Char('k') => self.scroll_detail(-1),
                    KeyCode::PageDown | KeyCode::Char(' ') => self.scroll_detail(self.half_page()),
                    KeyCode::PageUp => self.scroll_detail(-self.half_page()),
                    KeyCode::Home | KeyCode::Char('g') => {
                        self.detail_scroll = 0;
                        self.dirty = true;
                    }
                    KeyCode::End | KeyCode::Char('G') => {
                        self.detail_scroll = u16::MAX;
                        self.dirty = true;
                    }
                    _ => {}
                },
            },
        }
        false
    }
}

fn is_auth_error(message: &str) -> bool {
    message.contains("登录态已失效")
        || message.contains("未登录")
        || message.contains("401")
        || message.contains("403")
        || message.contains("访问被拒绝")
}

fn friendly_auth_message(message: &str) -> String {
    if message.contains("未登录") {
        "未登录 — 请选择扫码登录或粘贴 Cookie".to_string()
    } else if message.contains("登录态已失效") {
        "登录态已失效 — 请重新登录".to_string()
    } else {
        message.to_string()
    }
}
