//! 交互式终端界面。
//!
//! 用 `zhihu`（不带子命令）启动：左侧是热榜 / 推荐 / 搜索结果，右侧阅读正文。
//! 网络请求放在后台线程，界面不会因为接口慢而卡住。

mod app;
mod data;
mod shell;
mod ui;

use std::io::IsTerminal;
use std::time::Duration;

use anyhow::{bail, Result};
use ratatui::crossterm::event::{self, Event, KeyEventKind};

pub fn run() -> Result<()> {
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        bail!("TUI 需要在交互式终端中运行（stdout/stdin 不是 TTY）");
    }

    let mut app = app::App::new();
    let mut terminal = ratatui::init();

    let result = event_loop(&mut terminal, &mut app);
    ratatui::restore();
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut app::App) -> Result<()> {
    loop {
        app.poll();
        if app.dirty {
            terminal.draw(|frame| ui::draw(frame, app))?;
            app.dirty = false;
        }

        if event::poll(Duration::from_millis(100))? {
            match event::read()? {
                Event::Key(key) if key.kind == KeyEventKind::Press => {
                    if app.handle_key(key) {
                        break;
                    }
                }
                Event::Resize(_, _) => {
                    app.dirty = true;
                }
                _ => {}
            }
        }

        app.tick();
    }
    Ok(())
}
