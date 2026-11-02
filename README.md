# zhihu-cli（Rust 版）

用 Rust 编写的知乎命令行客户端：搜索、热榜、问题与回答、评论、推荐、用户资料、收藏、通知，以及发布提问 / 想法 / 文章。

直接运行 `zhihu`（不带子命令）会进入交互式 TUI；带子命令时用于脚本和逐条操作。

> 没有 `zhihu tui` 子命令，`zhihu` 就是 TUI 入口。

## 功能概览

- **交互式阅读**：热榜 / 推荐 / 搜索三个页签，左侧列表、右侧正文；网络请求在后台线程执行，切换和滚动不会卡住界面。
- **正文排版**：解析 HTML 的段落、标题、列表、引用、代码块、图片占位等结构，按终端宽度重新排版，而不是压成一整行。
- **问答阅读**：问题正文与前 10 条回答、回答正文与前 20 条评论、推荐流正文与评论、用户资料、话题介绍与热门问题。
- **账号登录**：终端二维码登录、粘贴浏览器 Cookie；CLI 与 TUI 共用同一份登录态。
- **互动创作**：赞同 / 取消赞同、关注问题、发布提问 / 想法 / 文章、删除自己的内容、查看收藏夹和通知。
- **TUI 快捷操作**：搜索、筛选、复制链接、浏览器打开、退出登录、快捷键帮助。

## 安装

需要 stable Rust 工具链。

```bash
git clone https://github.com/hjkl01/zhihu-cli.git
cd zhihu-cli

make build          # 构建 release，并复制到 ./zhihu
./zhihu --help
```

也可以直接使用 cargo：

```bash
cargo build --release
./target/release/zhihu --help

# 或安装到 ~/.cargo/bin
cargo install --path .
zhihu --help
```

如果 `cargo` 不在 `PATH` 中：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Makefile 目标：

| 命令 | 作用 |
| --- | --- |
| `make build` | 构建 release，并复制到 `./zhihu` |
| `make test` | 运行单元测试 |
| `make fmt` | 格式化代码 |
| `make clippy` | 运行 clippy 检查 |
| `make check` | 检查格式、clippy 和测试 |
| `make clean` | 清理构建产物和 `./zhihu` |

## 快速开始

```bash
# 首次使用：扫码登录
zhihu login --qrcode

# 进入 TUI
zhihu

# 或者使用子命令阅读热榜前 5 条，每条附带 1 条回答
zhihu hot -l 5 -a 1

# 检查本地登录态
zhihu status
```

## 登录与账号

```bash
# 二维码登录：终端显示二维码，同时保存 PNG
zhihu login --qrcode

# 直接粘贴浏览器 Cookie（必须包含 z_c0、_xsrf、d_c0）
zhihu login --cookie "z_c0=...; _xsrf=...; d_c0=..."

# 不带参数：本地登录态有效则直接通过；确认失效后重新扫码
zhihu login

zhihu status     # 只检查本地保存的 cookie，不联网
zhihu whoami     # 请求当前账号资料
zhihu logout     # 删除本地 cookie
```

说明：

- 二维码会在终端中绘制，并保存到 `~/.zhihu-cli/login_qrcode.png`；二维码会话约 2 分钟有效。
- `zhihu login --cookie` 会直接保存 Cookie，不做联网校验；如果 Cookie 无效，后续请求或 TUI 登录页会提示。
- Cookie 保存在 `~/.zhihu-cli/cookies.json`，权限为 `0600`，与 Python 版共用同一份文件。
- `status` 只判断本地文件是否存在且包含必需 Cookie；`whoami` 会实际请求知乎接口。

未登录或登录态失效时，TUI 会自动显示登录页：

| 按键 | 作用 |
| --- | --- |
| `1` / `Enter` | 扫码登录，在终端内显示二维码 |
| `2` / `c` | 粘贴浏览器 Cookie |
| `Enter` | Cookie 模式下提交并联网验证 |
| `Esc` | 返回登录菜单 / 取消二维码登录 |
| `r` | 二维码出错后重试 |
| `Ctrl-U` | 清空 Cookie 输入 |
| `q` / `Ctrl-C` | 退出 |

TUI 中 Cookie 登录会先验证；登录成功后自动回到热榜。二维码在终端显示异常时，可以使用 Cookie 方式。

## 交互式界面（TUI）

```bash
zhihu
```

- 终端宽度 **≥ 100 列**时显示两栏：左侧列表、右侧正文；窄于 100 列时列表和正文单栏切换。
- 列表数据在后台加载：热榜最多 50 条、推荐最多 20 条、搜索最多 20 条（`general` 搜索）。
- 打开问题会加载问题正文和前 10 条回答；打开回答会加载正文和前 20 条评论；用户和话题会加载资料 / 介绍。
- 列表、正文、评论都在后台线程加载，加载期间会显示 spinner，不会阻塞按键。
- 列表筛选会匹配标题、作者、元信息和摘要，输入时实时生效。

### 快捷键

| 按键 | 作用 |
| --- | --- |
| `1` / `2` / `3` | 切换到热榜 / 推荐 / 搜索页签 |
| `Tab` | 切换到下一个页签 |
| `↑` `↓` | 列表逐条移动；正文半页滚动 |
| `j` `k` | 列表逐条移动；正文单行滚动 |
| `Enter` / `→` / `l` | 打开选中项 |
| `Space` / `PgDn` / `PgUp` | 列表一次移动 10 条；正文半页滚动 |
| `g` / `G` | 列表跳到首 / 尾；正文跳到开头 / 结尾 |
| `/` | 输入关键词并搜索（回车后切到搜索页） |
| `f` | 在列表视图中筛选当前页签（`Esc` 清除） |
| `r` | 刷新当前页签列表 |
| `y` | 复制当前条目的网页链接 |
| `o` | 用系统默认浏览器打开当前条目 |
| `L` | 退出登录（需要按 `y` 确认） |
| `?` | 打开 / 关闭快捷键帮助 |
| `Esc` / `←` / `h` | 返回列表 / 关闭正文 / 清除筛选 |
| `Ctrl-d` / `Ctrl-u` | 正文向下 / 向上半页滚动 |
| `Ctrl-f` / `Ctrl-b` | 正文向下 / 向上整页滚动 |
| `q` / `Ctrl-C` | 退出 TUI |

TUI 需要 stdout 和 stdin 都是 TTY。通过管道运行时会直接报错：

```text
TUI 需要在交互式终端中运行（stdout/stdin 不是 TTY）
```

这种情况下请使用带子命令的 CLI 模式。

## 命令一览

```text
zhihu [OPTIONS] [COMMAND]
```

不带 `COMMAND` 时进入 TUI。全局选项：

- `-v` / `--verbose`：显示完整错误链；
- `-h` / `--help`：查看帮助；
- `-V` / `--version`：查看版本。

### 账号

| 命令 | 说明 |
| --- | --- |
| `zhihu login [--qrcode] [--cookie <COOKIE>]` | 扫码登录，或直接保存浏览器 Cookie |
| `zhihu logout` | 删除本地 Cookie |
| `zhihu status` | 检查本地登录态（不联网） |
| `zhihu whoami [--json]` | 查看当前账号资料 |

### 阅读与查询

| 命令 | 说明 |
| --- | --- |
| `zhihu search <QUERY> [-t general\|people\|topic] [-l 10] [-a 3] [--json]` | 搜索内容（问题 / 回答 / 文章，也可按 people / topic 搜索）；`-a` 为每个问题附带几条回答，`0` 表示不附带 |
| `zhihu hot [-l 50] [-a 3] [--json]` | 知乎热榜，可附带回答摘要 |
| `zhihu question <QUESTION_ID> [--json]` | 问题详情 |
| `zhihu answers <QUESTION_ID> [-l 5] [--sort default\|created] [--json]` | 问题下的回答列表 |
| `zhihu answer <ANSWER_ID> [-c] [-l 0] [--json]` | 阅读回答正文；`-c` 显示评论，`-l 0` 表示全部评论（默认值也是 `0`） |
| `zhihu feed [-l 10] [--json]` | 推荐列表，仅显示概要 |
| `zhihu feeds [-l 6] [-c 10] [--json]` | 推荐内容 + 正文 + 评论 |
| `zhihu topic <TOPIC_ID> [--json]` | 话题详情与热门问题 |
| `zhihu user <URL_TOKEN> [--json]` | 用户资料 |
| `zhihu user-answers <URL_TOKEN> [-l 10] [--json]` | 用户的回答 |
| `zhihu user-articles <URL_TOKEN> [-l 10] [--json]` | 用户的文章 |
| `zhihu followers <URL_TOKEN> [-l 10] [--json]` | 用户的粉丝 |
| `zhihu following <URL_TOKEN> [-l 10] [--json]` | 用户关注的人 |
| `zhihu collections [-l 10] [--json]` | 自己的收藏夹列表 |
| `zhihu notifications [-l 10] [--offset 0] [--json]` | 通知列表 |

除 `login` / `logout` / `status` 和写操作外，查询类命令都支持 `--json`，直接输出接口返回的 JSON。

### 互动与创作

| 命令 | 说明 |
| --- | --- |
| `zhihu vote <ANSWER_ID> [--neutral]` | 赞同回答；`--neutral` 取消赞同 |
| `zhihu follow-question <QUESTION_ID> [--unfollow]` | 关注问题；`--unfollow` 取消关注 |
| `zhihu ask <TITLE> [-d <DETAIL>] [-t <TOPIC_ID>]... [-i <IMAGE>]...` | 发布提问，描述支持 HTML |
| `zhihu pin <TITLE> [-c <CONTENT>] [-i <IMAGE>]...` | 发布想法，正文支持 HTML |
| `zhihu article <TITLE> <CONTENT> [-t <TOPIC_ID>]... [-i <IMAGE>]...` | 发布文章，正文支持 HTML |
| `zhihu delete-question <QUESTION_ID> [-y]` | 删除自己的提问 |
| `zhihu delete-pin <PIN_ID> [-y]` | 删除自己的想法 |
| `zhihu delete-article <ARTICLE_ID> [-y]` | 删除自己的文章 |

删除命令默认会要求输入 `y` 确认；加 `-y` / `--yes` 可跳过确认。提问、想法和文章命令中的 `-i` / `--image` 可以重复使用。

## 示例

```bash
# TUI
zhihu

# 热榜前 10 条，每条附带 2 条回答
zhihu hot -l 10 -a 2

# 搜索
zhihu search "Rust" -t general -l 5

# 按创建时间读取某个问题的回答
zhihu answers QUESTION_ID -l 5 --sort created

# 阅读回答及前 10 条评论（-l 0 为全部评论）
zhihu answer ANSWER_ID -c -l 10

# 查看推荐流正文，每条附带 5 条评论
zhihu feeds -l 3 -c 5

# 查看用户资料和回答
zhihu user URL_TOKEN
zhihu user-answers URL_TOKEN -l 5
```

发布内容：

```bash
zhihu ask "问题标题" \
  -d "问题描述，支持 <b>HTML</b>" \
  -t TOPIC_ID \
  -i ./image.png

zhihu pin "想法标题" \
  -c "想法正文，支持 <b>HTML</b>" \
  -i ./image.png

zhihu article "文章标题" \
  "文章正文，支持 HTML" \
  -t TOPIC_ID \
  -i ./image.png
```

## 排版与终端

- 优先读取 `COLUMNS`，其次读取真实终端宽度；都没有时按 100 列处理。
- 正文可用宽度为终端宽度减去 2 列边距，最小 40 列，最大 100 列。
- 例如 `COLUMNS=80 zhihu answer ANSWER_ID` 会按 78 列左右的正文宽度排版。
- `NO_COLOR=1` 可以关闭 ANSI 颜色；输出不是 TTY 时也不会启用颜色。
- 表格使用 `comfy-table` 的 UTF-8 边框，窄终端会自动压缩列宽，最大宽度为 160 列。
- 正文中的中文、英文、列表、引用和代码块都会按显示宽度换行。

## 常见问题

**请求返回 401 / 403？**

先检查登录态：

```bash
zhihu status
zhihu whoami
```

如果登录态失效，重新执行 `zhihu login --qrcode`。TUI 遇到 401/403 时会自动弹出登录页。403 也可能由请求过快或接口风控触发，可以降低频率后重试。

**二维码在终端里显示不正常？**

可以改用 Cookie 登录：

```bash
zhihu login --cookie "z_c0=...; _xsrf=...; d_c0=..."
```

或进入 TUI 后在登录页按 `2` 粘贴 Cookie。

**TUI 无法启动？**

确认是在真实终端中运行，且 stdout/stdin 没有被重定向或管道接管。

**复制或打开浏览器失败？**

`y` 依赖系统剪贴板命令（macOS 使用 `pbcopy`，Linux 使用 `wl-copy` / `xclip` / `xsel`）；`o` 在 macOS 使用 `open`，Linux 使用 `xdg-open`。缺少对应命令时会显示错误。

**需要更详细的错误信息？**

给任意命令加上全局选项：

```bash
zhihu -v hot -l 1
```

## 项目结构

```text
Makefile           构建、测试、检查、清理
src/
  main.rs          clap 命令定义与分发；无子命令时启动 TUI
  config.rs        URL、路径、浏览器指纹、必需 Cookie
  auth.rs          Cookie 存取、二维码登录会话（CLI / TUI 共用）
  client.rs        知乎 API 客户端
  html.rs          HTML → 终端文本渲染器（含单元测试）
  tui/
    mod.rs         TTY 检查、终端初始化与事件循环
    app.rs         应用状态、按键处理、后台任务
    data.rs        API JSON → 列表项 / 正文
    ui.rs          ratatui 两栏布局与渲染
    shell.rs       打开链接、系统剪贴板
  display.rs       颜色、计数、表格、终端宽度
  util.rs          serde_json 取值助手
  commands/
    auth.rs        login / logout / status / whoami
    content.rs     search / hot / question / answers / answer / feed / feeds / topic
    user.rs        用户资料与用户列表
    interact.rs    赞同 / 关注 / 发布 / 删除 / 收藏 / 通知
```

运行测试：

```bash
cargo test
# 或
make test
```

## 说明

- 本项目仅用于个人学习与命令行阅读，请遵守知乎的用户协议与 robots 规则，不要高频请求。
- 接口可能随知乎改版而变化；遇到登录失败或 403 时，先确认登录态，必要时重新登录或降低请求频率。
- 本项目在接口设计上参考了 Python 版 zhihu-cli，并重点重做了 HTML 正文排版和交互式 TUI。

## License

Apache-2.0，见 [LICENSE](LICENSE)。