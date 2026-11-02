//! TUI 界面绘制。

use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem as TuiListItem, Paragraph};
use ratatui::Frame;
use unicode_width::UnicodeWidthStr;

use crate::display;
use crate::html::{html_to_inline, render_html, wrap_line};
use crate::util::{json_str, ptr_str};

use super::app::{
    App, Detail, DetailCache, Focus, InputKind, LineKind, ListItem, LoginView, StyledLine, Tab,
};

const ACCENT: Color = Color::Cyan;
const DIM: Color = Color::DarkGray;
const SELECTED_BG: Color = Color::Rgb(0, 78, 90);

pub fn draw(frame: &mut Frame, app: &mut App) {
    let areas: [Rect; 3] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(frame.area());

    draw_header(frame, areas[0], app);

    if app.login.is_some() {
        draw_login(frame, areas[1], app);
        draw_footer(frame, areas[2], app);
        return;
    }

    let terminal_width = frame.area().width;
    if terminal_width >= 100 {
        let columns: [Rect; 2] =
            Layout::horizontal([Constraint::Percentage(40), Constraint::Percentage(60)])
                .areas(areas[1]);
        draw_list(frame, columns[0], app);
        draw_detail_pane(frame, columns[1], app);
    } else if app.focus == Focus::Detail && app.detail.is_some() {
        draw_detail_pane(frame, areas[1], app);
    } else {
        draw_list(frame, areas[1], app);
    }

    draw_footer(frame, areas[2], app);

    if app.confirm_logout {
        draw_confirm_logout(frame);
    }
    if app.help {
        draw_help(frame);
    }
}

fn draw_header(frame: &mut Frame, area: Rect, app: &App) {
    let mut spans = vec![Span::styled(
        " 知乎 TUI ",
        Style::default()
            .fg(Color::Black)
            .bg(ACCENT)
            .add_modifier(Modifier::BOLD),
    )];

    if app.login.is_some() {
        spans.push(Span::styled(
            " · 登录 ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        ));
        frame.render_widget(Paragraph::new(Line::from(spans)), area);
        return;
    }

    spans.push(Span::raw(" "));
    for (index, tab) in Tab::ALL.iter().enumerate() {
        let active = *tab == app.tab;
        let style = if active {
            Style::default()
                .fg(Color::Black)
                .bg(ACCENT)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(DIM)
        };
        spans.push(Span::styled(
            format!(" {} {} ", index + 1, tab.title()),
            style,
        ));
        spans.push(Span::raw(" "));
    }

    if app.tab == Tab::Search && !app.search_query.is_empty() {
        spans.push(Span::styled(
            format!("· {} ", app.search_query),
            Style::default().fg(Color::Gray),
        ));
    }
    if !app.active_filter().is_empty() {
        spans.push(Span::styled(
            format!("· 筛选: {} ", app.active_filter()),
            Style::default().fg(Color::Yellow),
        ));
    }
    if let Some(user) = app.user.as_ref() {
        spans.push(Span::styled(format!("· {user} "), Style::default().fg(DIM)));
    }

    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn draw_login(frame: &mut Frame, area: Rect, app: &App) {
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Line::from(Span::styled(
            " 登录知乎 ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let Some(login) = app.login.as_ref() else {
        return;
    };

    match login {
        LoginView::Menu { message } => {
            let mut lines = vec![
                Line::from(Span::styled(
                    "未登录，或登录态已失效",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(vec![
                    Span::styled("1", Style::default().fg(ACCENT)),
                    Span::raw(" / "),
                    Span::styled("Enter", Style::default().fg(ACCENT)),
                    Span::raw("  扫码登录（推荐）"),
                ]),
                Line::from(vec![
                    Span::styled("2", Style::default().fg(ACCENT)),
                    Span::raw(" / "),
                    Span::styled("c", Style::default().fg(ACCENT)),
                    Span::raw("      粘贴浏览器 Cookie"),
                ]),
                Line::from(""),
            ];
            if let Some(message) = message {
                for line in wrap_text(message, inner.width.saturating_sub(4) as usize) {
                    lines.push(Line::from(Span::styled(line, Style::default().fg(DIM))));
                }
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                "提示：二维码在终端内显示；若显示异常，可按 2 粘贴 Cookie。",
                Style::default().fg(DIM),
            )));
            let paragraph = Paragraph::new(lines).alignment(Alignment::Center);
            frame.render_widget(paragraph, inner);
        }
        LoginView::Qr {
            lines,
            link,
            status,
            error,
            ..
        } => {
            let chunks: [Rect; 2] =
                Layout::vertical([Constraint::Min(1), Constraint::Length(5)]).areas(inner);
            if lines.is_empty() {
                draw_centered(
                    frame,
                    chunks[0],
                    &format!("{} 正在获取二维码…", app.spinner_text()),
                    DIM,
                );
            } else {
                let qr_height = chunks[0].height as usize;
                let visible = lines.len().min(qr_height);
                let mut text = Text::default();
                for line in lines.iter().take(visible) {
                    text.push_line(Line::from(Span::styled(
                        line.clone(),
                        Style::default().fg(Color::White),
                    )));
                }
                frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), chunks[0]);
            }

            let mut info = vec![Line::from(Span::styled(
                "请用知乎 App 扫码，并在手机上确认登录",
                Style::default().fg(Color::Yellow),
            ))];
            if !link.is_empty() {
                info.push(Line::from(Span::styled(
                    link.clone(),
                    Style::default().fg(DIM),
                )));
            }
            info.push(Line::from(Span::styled(
                status.clone(),
                Style::default().fg(ACCENT),
            )));
            if let Some(error) = error {
                info.push(Line::from(Span::styled(
                    format!("{error}（Esc 返回，r 重试）"),
                    Style::default().fg(Color::LightRed),
                )));
            } else {
                info.push(Line::from(Span::styled(
                    "Esc 返回 · q 退出",
                    Style::default().fg(DIM),
                )));
            }
            frame.render_widget(
                Paragraph::new(info)
                    .alignment(Alignment::Center)
                    .wrap(ratatui::widgets::Wrap { trim: false }),
                chunks[1],
            );
        }
        LoginView::Cookie {
            buffer,
            cursor,
            error,
            verifying,
        } => {
            let mut lines = vec![
                Line::from(Span::styled(
                    "粘贴浏览器 Cookie",
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "从浏览器开发者工具 → Network → 任意知乎请求 → Request Headers 复制 Cookie。",
                    Style::default().fg(DIM),
                )),
                Line::from(Span::styled(
                    "必须包含 z_c0、_xsrf、d_c0 三个字段。",
                    Style::default().fg(DIM),
                )),
                Line::from(""),
            ];
            if *verifying {
                lines.push(Line::from(Span::styled(
                    format!("{} 正在验证 Cookie…", app.spinner_text()),
                    Style::default().fg(ACCENT),
                )));
            } else {
                let width = inner.width.saturating_sub(4) as usize;
                let (before, after, clipped_left, clipped_right) =
                    input_window(buffer, *cursor, width);
                let mut spans = vec![Span::styled("> ", Style::default().fg(ACCENT))];
                if clipped_left {
                    spans.push(Span::styled("…", Style::default().fg(DIM)));
                }
                spans.push(Span::styled(before, Style::default().fg(Color::White)));
                spans.push(Span::styled("▏", Style::default().fg(ACCENT)));
                spans.push(Span::styled(after, Style::default().fg(Color::White)));
                if clipped_right {
                    spans.push(Span::styled("…", Style::default().fg(DIM)));
                }
                lines.push(Line::from(spans));
            }
            if let Some(error) = error {
                lines.push(Line::from(""));
                lines.push(Line::from(Span::styled(
                    error.clone(),
                    Style::default().fg(Color::LightRed),
                )));
            }
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "Enter 验证 · Esc 返回 · Ctrl-U 清空",
                Style::default().fg(DIM),
            )));
            frame.render_widget(
                Paragraph::new(lines)
                    .wrap(ratatui::widgets::Wrap { trim: false })
                    .block(Block::default().padding(ratatui::widgets::Padding::new(1, 1, 1, 0))),
                inner,
            );
        }
    }
}

fn draw_confirm_logout(frame: &mut Frame) {
    let area = centered_fixed(50, 7, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightRed))
        .title(" 退出登录 ");
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let text = vec![
        Line::from(Span::styled(
            "确定删除本地登录 Cookie？",
            Style::default().fg(Color::White),
        )),
        Line::from(""),
        Line::from(vec![
            Span::styled("y", Style::default().fg(Color::LightRed)),
            Span::raw(" 确认    "),
            Span::styled("n / Esc", Style::default().fg(ACCENT)),
            Span::raw(" 取消"),
        ]),
    ];
    frame.render_widget(Paragraph::new(text).alignment(Alignment::Center), inner);
}

fn draw_help(frame: &mut Frame) {
    // 18 行快捷键 + 上下边框；小终端下自动收缩。
    let area = centered_fixed(76, 20, frame.area());
    frame.render_widget(Clear, area);
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(ACCENT))
        .title(Line::from(Span::styled(
            " 快捷键 ",
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let rows = [
        ("1 / 2 / 3", "切换热榜 / 推荐 / 搜索"),
        ("Tab", "下一个标签"),
        ("↑ ↓", "列表选择；正文半页"),
        ("j / k", "列表选择；正文单行"),
        ("Enter / → / l", "阅读正文"),
        ("Space / PgDn / PgUp", "列表翻页；正文半页"),
        ("g / G", "跳到顶部 / 底部"),
        ("/", "搜索"),
        ("f", "筛选当前列表"),
        ("r", "刷新"),
        ("y", "复制当前链接"),
        ("o", "浏览器打开"),
        ("Esc / ← / h", "返回 / 关闭正文 / 清除筛选"),
        ("Ctrl-d / Ctrl-u", "正文半页向下 / 向上"),
        ("Ctrl-f / Ctrl-b", "正文整页向下 / 向上"),
        ("L", "退出登录"),
        ("?", "打开 / 关闭本帮助"),
        ("q / Ctrl-C", "退出"),
    ];
    let lines: Vec<Line> = rows
        .iter()
        .map(|(key, desc)| {
            Line::from(vec![
                Span::styled(format!("{key:<20}"), Style::default().fg(ACCENT)),
                Span::styled(*desc, Style::default().fg(Color::White)),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(lines)
            .block(Block::default().padding(ratatui::widgets::Padding::new(2, 1, 0, 0))),
        inner,
    );
}

/// 在固定宽度内显示光标附近的文本，返回 (光标前, 光标后, 左侧省略, 右侧省略)。
fn input_window(text: &str, cursor: usize, width: usize) -> (String, String, bool, bool) {
    let chars: Vec<char> = text.chars().collect();
    let width = width.max(8);
    if chars.len() <= width {
        return (
            chars[..cursor.min(chars.len())].iter().collect(),
            chars[cursor.min(chars.len())..].iter().collect(),
            false,
            false,
        );
    }

    let available = width.saturating_sub(2);
    let start = cursor.saturating_sub(available);
    let end = (start + available).min(chars.len());
    let start = end.saturating_sub(available);
    let before = chars[start..cursor.min(end)].iter().collect();
    let after = chars[cursor.min(end)..end].iter().collect();
    (before, after, start > 0, end < chars.len())
}

fn wrap_text(text: &str, width: usize) -> Vec<String> {
    let width = width.max(10);
    let mut out = Vec::new();
    for raw in text.lines() {
        let mut current = String::new();
        let mut current_width = 0usize;
        for ch in raw.chars() {
            let w = unicode_width::UnicodeWidthChar::width(ch).unwrap_or(1);
            if current_width + w > width && !current.is_empty() {
                out.push(current);
                current = String::new();
                current_width = 0;
            }
            current.push(ch);
            current_width += w;
        }
        out.push(current);
    }
    out
}

fn centered_fixed(width: u16, height: u16, area: Rect) -> Rect {
    let width = width.min(area.width);
    let height = height.min(area.height);
    Rect {
        x: area.x + (area.width - width) / 2,
        y: area.y + (area.height - height) / 2,
        width,
        height,
    }
}

fn draw_list(frame: &mut Frame, area: Rect, app: &mut App) {
    let border_style = if app.focus == Focus::List {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(DIM)
    };
    let title = if app.active_filter().is_empty() {
        format!(" {} · {} 条 ", app.tab.title(), app.current_list().len())
    } else {
        format!(
            " {} · {}/{} 条 ",
            app.tab.title(),
            app.current_list().len(),
            app.total_count()
        )
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Line::from(vec![Span::styled(
            title,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )]));

    let inner_width = area.width.saturating_sub(2).max(1) as usize;
    let text_width = inner_width.saturating_sub(6).max(6);
    let items: Vec<TuiListItem> = app
        .current_list()
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let title = display::truncate(&item.title, text_width);
            let meta = if item.author.is_empty() {
                format!("{} · {}", item.kind.label(), item.id)
            } else {
                format!("{} · {}", item.kind.label(), item.author)
            };
            let meta = display::truncate(&meta, text_width);
            TuiListItem::new(vec![
                Line::from(vec![
                    Span::styled(format!("{:>2}. ", index + 1), Style::default().fg(DIM)),
                    Span::styled(title, Style::default().fg(Color::White)),
                ]),
                Line::from(vec![
                    Span::raw("    "),
                    Span::styled(meta, Style::default().fg(DIM)),
                ]),
            ])
        })
        .collect();

    let list = List::new(items)
        .block(block)
        .highlight_symbol("▌ ")
        .highlight_style(
            Style::default()
                .bg(SELECTED_BG)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    app.list_state.select(Some(app.selected_index()));
    frame.render_stateful_widget(list, area, &mut app.list_state);

    if app.current_list().is_empty() {
        let text = if app.list_loading[app.tab.index()] {
            format!("{} 加载中…", app.spinner_text())
        } else if !app.active_filter().is_empty() {
            "没有匹配条目\n按 Esc 清除筛选".to_string()
        } else if app.tab == Tab::Search && app.search_query.is_empty() {
            "按 / 输入关键词搜索".to_string()
        } else {
            "暂无内容\n按 / 搜索，或 r 刷新".to_string()
        };
        let paragraph = Paragraph::new(text)
            .alignment(Alignment::Center)
            .style(Style::default().fg(DIM));
        frame.render_widget(paragraph, inner_area(area));
    }
}

fn draw_detail_pane(frame: &mut Frame, area: Rect, app: &mut App) {
    let border_style = if app.focus == Focus::Detail {
        Style::default().fg(ACCENT)
    } else {
        Style::default().fg(DIM)
    };
    let title = if app.detail_loading {
        format!(" {} 读取中 ", app.spinner_text())
    } else {
        " 正文 ".to_string()
    };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(border_style)
        .title(Line::from(Span::styled(
            title,
            Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        )));
    let inner = block.inner(area);
    app.detail_view_height = inner.height;
    frame.render_widget(block, area);

    if app.detail_loading {
        draw_centered(
            frame,
            inner,
            &format!("{} 正在读取…", app.spinner_text()),
            DIM,
        );
        return;
    }

    let content_width = inner.width.saturating_sub(2).max(20) as usize;
    let rebuild = app
        .detail_cache
        .as_ref()
        .map(|cache| cache.width as usize != content_width || cache.generation != app.detail_gen)
        .unwrap_or(true);
    if rebuild {
        let generation = app.detail_gen;
        app.detail_cache = app.detail.as_ref().map(|detail| DetailCache {
            width: content_width as u16,
            generation,
            lines: build_detail_lines(detail, content_width),
        });
    }

    if let Some(cache) = app.detail_cache.as_ref() {
        let max_scroll = render_styled_lines(frame, inner, &cache.lines, app.detail_scroll);
        if app.detail_scroll > max_scroll {
            app.detail_scroll = max_scroll;
        }
    } else if let Some(item) = app.current_item() {
        let lines = preview_lines(item, content_width);
        let _ = render_styled_lines(frame, inner, &lines, 0);
    } else {
        draw_centered(frame, inner, "没有选中内容", DIM);
    }
}

fn draw_footer(frame: &mut Frame, area: Rect, app: &App) {
    let (left, left_style) = if app.login.is_some() {
        let hints = match app.login.as_ref() {
            Some(LoginView::Menu { .. }) => " 1 扫码登录 · 2 粘贴 Cookie · q 退出",
            Some(LoginView::Qr { .. }) => " Esc 返回 · q 退出",
            Some(LoginView::Cookie { .. }) => " Enter 验证 · Esc 返回 · Ctrl-U 清空",
            None => "",
        };
        (hints.to_string(), Style::default().fg(DIM))
    } else if let Some(input) = app.input.as_ref() {
        let (before, after) = input.split();
        let prefix = match input.kind {
            InputKind::Search => " /",
            InputKind::Filter => " 筛选",
        };
        let suffix = match input.kind {
            InputKind::Search => "  Enter 搜索 · Esc 取消",
            InputKind::Filter => "  Enter 应用 · Esc 清除",
        };
        (
            format!("{prefix} {before}▏{after}{suffix}"),
            Style::default().fg(Color::White),
        )
    } else {
        let hints = match app.focus {
            Focus::Detail => " ↑↓/Space 半页 · j/k 单行 · Esc/←/h 返回 · y 复制 · o 打开 · ? 帮助",
            Focus::List => {
                " ↑↓ 选择 · Enter/→/l 阅读 · 1/2/3 切换 · / 搜索 · f 筛选 · r 刷新 · ? 帮助"
            }
        };
        (hints.to_string(), Style::default().fg(DIM))
    };

    let (status, status_style) = if let Some(status) = app.status.as_ref() {
        let style = if status.error {
            Style::default().fg(Color::LightRed)
        } else {
            Style::default().fg(Color::Yellow)
        };
        (status.text.clone(), style)
    } else if matches!(
        app.login,
        Some(LoginView::Cookie {
            verifying: true,
            ..
        })
    ) {
        (
            format!("{} 验证中", app.spinner_text()),
            Style::default().fg(DIM),
        )
    } else if matches!(app.login, Some(LoginView::Qr { .. })) {
        (
            format!("{} 登录中", app.spinner_text()),
            Style::default().fg(DIM),
        )
    } else if app.detail_loading {
        (
            format!("{} 读取中", app.spinner_text()),
            Style::default().fg(DIM),
        )
    } else if app.list_loading[app.tab.index()] {
        (
            format!("{} 加载中", app.spinner_text()),
            Style::default().fg(DIM),
        )
    } else {
        (String::new(), Style::default())
    };

    let width = area.width as usize;
    let status_width = status.width();
    let left_max = width.saturating_sub(status_width + 1);
    let left = display::truncate(&left, left_max);
    let padding = width.saturating_sub(left.width() + status_width);
    let line = Line::from(vec![
        Span::styled(left, left_style),
        Span::raw(" ".repeat(padding)),
        Span::styled(status, status_style),
    ]);
    frame.render_widget(Paragraph::new(line), area);
}

fn inner_area(area: Rect) -> Rect {
    Rect {
        x: area.x.saturating_add(1),
        y: area.y.saturating_add(1),
        width: area.width.saturating_sub(2),
        height: area.height.saturating_sub(2),
    }
}

fn draw_centered(frame: &mut Frame, area: Rect, text: &str, color: Color) {
    if area.height == 0 {
        return;
    }
    let mut lines = Vec::new();
    for _ in 0..area.height / 2 {
        lines.push(Line::default());
    }
    lines.push(Line::from(Span::styled(
        text.to_string(),
        Style::default().fg(color),
    )));
    let paragraph = Paragraph::new(Text::from(lines)).alignment(Alignment::Center);
    frame.render_widget(paragraph, area);
}

fn style_for(kind: LineKind) -> Style {
    match kind {
        LineKind::Title => Style::default().fg(ACCENT).add_modifier(Modifier::BOLD),
        LineKind::Heading => Style::default()
            .fg(Color::LightGreen)
            .add_modifier(Modifier::BOLD),
        LineKind::Meta => Style::default().fg(DIM),
        LineKind::Body => Style::default(),
        LineKind::Comment => Style::default().fg(Color::Gray),
        LineKind::Blank => Style::default(),
    }
}

fn render_styled_lines(frame: &mut Frame, area: Rect, lines: &[StyledLine], scroll: u16) -> u16 {
    let max_scroll = (lines.len() as u16).saturating_sub(area.height);
    let offset = scroll.min(max_scroll);
    let lines: Vec<Line> = lines
        .iter()
        .map(|line| {
            if line.kind == LineKind::Blank {
                Line::default()
            } else {
                Line::from(Span::styled(line.text.clone(), style_for(line.kind)))
            }
        })
        .collect();
    let paragraph = Paragraph::new(Text::from(lines)).scroll((offset, 0));
    frame.render_widget(paragraph, area);
    max_scroll
}

fn push_blank(out: &mut Vec<StyledLine>) {
    out.push(StyledLine {
        text: String::new(),
        kind: LineKind::Blank,
    });
}

fn push_text(out: &mut Vec<StyledLine>, text: &str, width: usize, kind: LineKind, indent: usize) {
    let width = width.saturating_sub(indent).max(4);
    for line in wrap_line(text, width) {
        out.push(StyledLine {
            text: format!("{}{}", " ".repeat(indent), line),
            kind,
        });
    }
}

fn push_html(out: &mut Vec<StyledLine>, html: &str, width: usize, indent: usize) {
    if html.trim().is_empty() {
        return;
    }
    let rendered = render_html(html, width.saturating_sub(indent).max(20));
    for line in rendered.lines() {
        if line.is_empty() {
            push_blank(out);
        } else {
            out.push(StyledLine {
                text: format!("{}{}", " ".repeat(indent), line),
                kind: LineKind::Body,
            });
        }
    }
}

fn push_stats(out: &mut Vec<StyledLine>, stats: &[(String, String)]) {
    let text = stats
        .iter()
        .map(|(label, value)| format!("{value} {label}"))
        .collect::<Vec<_>>()
        .join(" · ");
    if !text.is_empty() {
        out.push(StyledLine {
            text: format!("  {text}"),
            kind: LineKind::Meta,
        });
    }
}

fn build_detail_lines(detail: &Detail, width: usize) -> Vec<StyledLine> {
    let mut out = Vec::new();
    match detail {
        Detail::Question { question, answers } => {
            let title = html_to_inline(json_str(question, "title"));
            if !title.is_empty() {
                push_text(&mut out, &title, width, LineKind::Title, 2);
            }

            let author = ptr_str(question, "/author/name");
            if !author.is_empty() {
                out.push(StyledLine {
                    text: format!("  作者：{author}"),
                    kind: LineKind::Meta,
                });
            }
            let mut stats = Vec::new();
            if let Some(v) = question.get("answer_count") {
                stats.push(("回答".to_string(), display::format_count(Some(v))));
            }
            if let Some(v) = question.get("follower_count") {
                stats.push(("关注".to_string(), display::format_count(Some(v))));
            }
            if let Some(v) = question.get("visit_count") {
                stats.push(("浏览".to_string(), display::format_count(Some(v))));
            }
            push_stats(&mut out, &stats);
            push_blank(&mut out);

            let detail = json_str(question, "detail");
            if !detail.trim().is_empty() {
                push_html(&mut out, detail, width, 2);
                push_blank(&mut out);
            }

            if answers.is_empty() {
                out.push(StyledLine {
                    text: "  暂无回答".to_string(),
                    kind: LineKind::Meta,
                });
                return out;
            }

            out.push(StyledLine {
                text: format!("  回答（{}）", answers.len()),
                kind: LineKind::Heading,
            });
            push_blank(&mut out);

            for (index, answer) in answers.iter().enumerate() {
                let author = {
                    let name = ptr_str(answer, "/author/name");
                    if name.is_empty() {
                        "匿名用户".to_string()
                    } else {
                        name.to_string()
                    }
                };
                out.push(StyledLine {
                    text: format!("  {}. {}", index + 1, author),
                    kind: LineKind::Heading,
                });
                let mut stats = Vec::new();
                if let Some(v) = answer.get("voteup_count") {
                    stats.push(("赞同".to_string(), display::format_count(Some(v))));
                }
                if let Some(v) = answer.get("comment_count") {
                    stats.push(("评论".to_string(), display::format_count(Some(v))));
                }
                push_stats(&mut out, &stats);

                let content = {
                    let content = json_str(answer, "content");
                    if content.is_empty() {
                        json_str(answer, "excerpt")
                    } else {
                        content
                    }
                };
                push_html(&mut out, content, width, 2);
                push_blank(&mut out);
            }
        }
        Detail::Answer { answer, comments } => {
            let question_title = ptr_str(answer, "/question/title");
            if !question_title.is_empty() {
                push_text(
                    &mut out,
                    &html_to_inline(question_title),
                    width,
                    LineKind::Title,
                    2,
                );
            }
            let author = {
                let name = ptr_str(answer, "/author/name");
                if name.is_empty() {
                    "匿名用户".to_string()
                } else {
                    name.to_string()
                }
            };
            out.push(StyledLine {
                text: format!("  {author} 的回答"),
                kind: LineKind::Heading,
            });
            let mut stats = Vec::new();
            if let Some(v) = answer.get("voteup_count") {
                stats.push(("赞同".to_string(), display::format_count(Some(v))));
            }
            if let Some(v) = answer.get("comment_count") {
                stats.push(("评论".to_string(), display::format_count(Some(v))));
            }
            push_stats(&mut out, &stats);
            push_blank(&mut out);

            push_html(&mut out, json_str(answer, "content"), width, 2);
            push_blank(&mut out);

            if comments.is_empty() {
                out.push(StyledLine {
                    text: "  暂无评论".to_string(),
                    kind: LineKind::Meta,
                });
                return out;
            }

            out.push(StyledLine {
                text: "  评论".to_string(),
                kind: LineKind::Heading,
            });
            push_blank(&mut out);
            for (index, comment) in comments.iter().enumerate() {
                let content = html_to_inline(json_str(comment, "content"));
                let likes = display::format_count(comment.get("vote_count"));
                let marker = format!("{}. ", index + 1);
                let text_width = width.saturating_sub(6).max(8);
                for (line_index, line) in wrap_line(&content, text_width).iter().enumerate() {
                    let text = if line_index == 0 {
                        format!("    {}{}", marker, line)
                    } else {
                        format!("       {}", line)
                    };
                    out.push(StyledLine {
                        text,
                        kind: LineKind::Comment,
                    });
                }
                out.push(StyledLine {
                    text: format!("       {} 赞", likes),
                    kind: LineKind::Meta,
                });
                push_blank(&mut out);
            }
        }
        Detail::Raw {
            title,
            author,
            meta,
            html,
            kind,
        } => {
            if !title.is_empty() {
                push_text(&mut out, title, width, LineKind::Title, 2);
            }
            let mut info = vec![kind.label().to_string()];
            if !author.is_empty() {
                info.push(author.clone());
            }
            if !meta.is_empty() {
                info.push(meta.clone());
            }
            out.push(StyledLine {
                text: format!("  {}", info.join(" · ")),
                kind: LineKind::Meta,
            });
            push_blank(&mut out);
            if html.trim().is_empty() {
                out.push(StyledLine {
                    text: "  暂无正文".to_string(),
                    kind: LineKind::Meta,
                });
            } else {
                push_html(&mut out, html, width, 2);
            }
        }
        Detail::Profile { user } => {
            let name = json_str(user, "name");
            if !name.is_empty() {
                push_text(&mut out, name, width, LineKind::Title, 2);
            }
            let headline = html_to_inline(json_str(user, "headline"));
            if !headline.is_empty() {
                out.push(StyledLine {
                    text: format!("  {headline}"),
                    kind: LineKind::Meta,
                });
            }
            push_blank(&mut out);
            let description = json_str(user, "description");
            if !description.trim().is_empty() {
                push_html(&mut out, description, width, 2);
                push_blank(&mut out);
            }
            let mut stats = Vec::new();
            for (key, label) in [
                ("answer_count", "回答"),
                ("articles_count", "文章"),
                ("follower_count", "粉丝"),
                ("following_count", "关注"),
            ] {
                if let Some(v) = user.get(key) {
                    stats.push((label.to_string(), display::format_count(Some(v))));
                }
            }
            push_stats(&mut out, &stats);
        }
        Detail::Topic { topic } => {
            let name = json_str(topic, "name");
            if !name.is_empty() {
                push_text(&mut out, &format!("# {name}"), width, LineKind::Title, 2);
            }
            push_blank(&mut out);
            let introduction = json_str(topic, "introduction");
            if !introduction.trim().is_empty() {
                push_html(&mut out, introduction, width, 2);
                push_blank(&mut out);
            }
            let mut stats = Vec::new();
            for (key, label) in [("followers_count", "关注"), ("questions_count", "问题")] {
                if let Some(v) = topic.get(key) {
                    stats.push((label.to_string(), display::format_count(Some(v))));
                }
            }
            push_stats(&mut out, &stats);
        }
    }
    out
}

fn preview_lines(item: &ListItem, width: usize) -> Vec<StyledLine> {
    let mut out = Vec::new();
    push_text(&mut out, &item.title, width, LineKind::Title, 2);
    let mut info = vec![item.kind.label().to_string()];
    if !item.author.is_empty() {
        info.push(item.author.clone());
    }
    if !item.meta.is_empty() {
        info.push(item.meta.clone());
    }
    out.push(StyledLine {
        text: format!("  {}", info.join(" · ")),
        kind: LineKind::Meta,
    });
    push_blank(&mut out);
    if item.excerpt.is_empty() {
        out.push(StyledLine {
            text: "  暂无摘要".to_string(),
            kind: LineKind::Meta,
        });
    } else {
        push_text(&mut out, &item.excerpt, width, LineKind::Body, 2);
    }
    push_blank(&mut out);
    out.push(StyledLine {
        text: "  Enter 阅读全文 · / 搜索".to_string(),
        kind: LineKind::Meta,
    });
    out
}
