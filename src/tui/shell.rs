//! 与桌面环境交互的小工具：打开链接、复制到剪贴板。

use std::io::Write;
use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

/// 用系统默认浏览器打开链接。
pub fn open_url(url: &str) -> Result<()> {
    if url.is_empty() {
        bail!("链接为空");
    }

    #[cfg(target_os = "macos")]
    let mut child = Command::new("open")
        .arg(url)
        .spawn()
        .context("启动 open 失败")?;

    #[cfg(target_os = "windows")]
    let mut child = Command::new("cmd")
        .args(["/C", "start", "", url])
        .spawn()
        .context("启动 start 失败")?;

    #[cfg(all(unix, not(target_os = "macos")))]
    let mut child = Command::new("xdg-open")
        .arg(url)
        .spawn()
        .context("启动 xdg-open 失败")?;

    let status = child.wait().context("等待浏览器启动失败")?;
    if !status.success() {
        bail!("打开链接失败（退出码 {:?}）", status.code());
    }
    Ok(())
}

/// 复制文本到系统剪贴板。
pub fn copy_text(text: &str) -> Result<()> {
    #[cfg(target_os = "macos")]
    {
        write_to("pbcopy", &[], text)
    }

    #[cfg(target_os = "windows")]
    {
        write_to("clip", &[], text)
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    {
        let candidates: [(&str, &[&str]); 3] = [
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ];
        for (command, args) in candidates {
            if write_to(command, args, text).is_ok() {
                return Ok(());
            }
        }
        bail!("未找到剪贴板命令（可安装 wl-clipboard 或 xclip）")
    }
}

fn write_to(command: &str, args: &[&str], text: &str) -> Result<()> {
    let mut child = Command::new(command)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .with_context(|| format!("启动 {command} 失败"))?;
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).context("写入剪贴板失败")?;
    }
    let status = child.wait().context("等待剪贴板命令失败")?;
    if !status.success() {
        bail!("{command} 退出码 {:?}", status.code());
    }
    Ok(())
}
