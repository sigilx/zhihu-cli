# zhihu-cli（Rust 版）

用 Rust 重写的知乎命令行工具：在终端里搜索、阅读、互动，不用打开浏览器。

本项目参考了 Python 版 zhihu-cli 的接口与命令设计，并重点重做了**正文渲染**：HTML 会被解析成段落、标题、列表、引用、代码块，再按终端宽度重新排版，而不是压成一整行。

## 正文渲染

Python 版直接把 HTML 去标签后输出，段落和换行会丢失。Rust 版采用两阶段渲染：

1. **块级解析**：`<p>`、`<h1>`–`<h6>`、`<ul>/<ol>`、`<blockquote>`、`<pre>`、`<figure>`、表格、分隔线；
2. **终端排版**：块之间空行，引用加 `│`，列表加 `•` / `1.` 悬挂缩进，代码块保留缩进，中英文混排按显示宽度换行。

效果示例：

```
  耐心等待 的回答

  OpenAI联合创始人Greg Brockman的答复：Rust是开发智能体的完美语言。 只要能
  编译通过，代码就基本是正确的。

  据世界编程语言排行榜TIOBE 2026年7月最新数据，Rust语言成功进入前十。在AI
  助力下，鉴于系统安全和运行性能需求，预计更多开源项目将向Rust重构。

  ▸ 69 赞同  ▸ 30 评论
```

阅读回答时加 `--comments` / `-c`，评论也会按同样宽度换行并对齐：

```
  评论

    1. rust的安全高效是有成本的，到这部分成本恰恰又是ai编程能够覆盖的[感谢]
       0 赞
    2. 只要能编译通过，代码就基本是正确的 这话是个程序员都不会说，算法有漏洞，
       代码就不会正确。
       11 赞
```

## 安装

需要 Rust 工具链（stable）：

```bash
make build          # 构建 release，并复制到 ./zhihu
./zhihu --help
```

也可以直接用 cargo 构建：

```bash
cargo build --release
./target/release/zhihu --help
```

或安装到 `~/.cargo/bin`：

```bash
cargo install --path .
zhihu --help
```

其他 Makefile 目标：

```bash
make help           # 查看所有目标
make test           # 运行测试
make check          # 格式 + clippy + 测试
make clean          # 清理构建产物和 ./zhihu
```

如果 `cargo` 不在 PATH 中：

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

## 登录

```bash
# 二维码登录（终端里直接显示二维码，同时保存 PNG）
zhihu login --qrcode

# 直接粘贴浏览器 Cookie（必须包含 z_c0、_xsrf、d_c0）
zhihu login --cookie "z_c0=...; _xsrf=...; d_c0=..."

zhihu status     # 检查本地登录态（不联网）
zhihu whoami     # 查看当前账号资料
zhihu logout     # 删除本地 cookie
```

Cookie 保存在 `~/.zhihu-cli/cookies.json`（权限 `0600`），与 Python 版共用同一份文件。

## 命令一览

### 阅读

| 命令 | 说明 |
| --- | --- |
| `zhihu search <关键词> [-t general\|people\|topic] [-l 10] [-a 3] [--json]` | 搜索问题、回答、文章 |
| `zhihu hot [-l 50] [-a 3] [--json]` | 知乎热榜，可附带回答摘要 |
| `zhihu question <问题ID> [--json]` | 问题详情 |
| `zhihu answers <问题ID> [-l 5] [--sort default\|created] [--json]` | 问题下的回答列表 |
| `zhihu answer <回答ID> [-c] [-l N] [--json]` | 阅读回答正文；`-c` 显示评论，`-l 0` 表示全部评论 |
| `zhihu feed [-l 10] [--json]` | 推荐列表（仅概要） |
| `zhihu feeds [-l 6] [-c 10] [--json]` | 推荐内容 + 正文 + 评论 |
| `zhihu topic <话题ID> [--json]` | 话题详情与热门问题 |

### 用户

| 命令 | 说明 |
| --- | --- |
| `zhihu user <url_token> [--json]` | 用户资料 |
| `zhihu user-answers <url_token> [-l 10] [--json]` | 用户的回答 |
| `zhihu user-articles <url_token> [-l 10] [--json]` | 用户的文章 |
| `zhihu followers <url_token> [-l 10] [--json]` | 粉丝 |
| `zhihu following <url_token> [-l 10] [--json]` | 关注的人 |

### 互动与创作

| 命令 | 说明 |
| --- | --- |
| `zhihu vote <回答ID> [--neutral]` | 赞同 / 取消赞同 |
| `zhihu follow-question <问题ID> [--unfollow]` | 关注 / 取消关注问题 |
| `zhihu ask <标题> [-d 描述] [-t 话题ID]... [-i 图片]...` | 发布提问，描述支持 HTML |
| `zhihu pin <标题> [-c 正文] [-i 图片]...` | 发布想法 |
| `zhihu article <标题> <正文> [-t 话题ID]... [-i 图片]...` | 发布文章，正文支持 HTML |
| `zhihu delete-question <问题ID> [-y]` | 删除自己的提问 |
| `zhihu delete-pin <想法ID> [-y]` | 删除自己的想法 |
| `zhihu delete-article <文章ID> [-y]` | 删除自己的文章 |
| `zhihu collections [-l 10] [--json]` | 收藏夹列表 |
| `zhihu notifications [-l 10] [--offset 0] [--json]` | 通知 |

所有只读命令都支持 `--json` 输出原始接口数据（赞同、发布、删除等互动命令除外）。全局参数 `-v/--verbose` 会打印完整错误链。

## 示例

```bash
# 看热榜前 5 条，每条带 1 个回答
zhihu hot -l 5 -a 1

# 搜索并直接读某个问题的回答列表
zhihu search "Rust" -l 3 -a 2
zhihu answers 2065849699827987563 -l 5 --sort created

# 读回答 + 前 10 条评论
zhihu answer 2068101524891480592 -c -l 10

# 发布一条带图片的想法
zhihu pin "今天试了试 Rust 重写的知乎 CLI" \
  -c "正文支持 <b>HTML</b>" \
  -i ./screenshot.png

# 发布提问
zhihu ask "Rust 适合写 CLI 吗？" \
  -d "想听听大家的实际体验。" \
  -t 19550517
```

## 排版相关

- 默认读取终端宽度；输出被管道接走时回退到约 100 列。
- 可以用 `COLUMNS=80 zhihu answer ...` 强制指定宽度（便于截图或测试）。
- 设置 `NO_COLOR=1` 关闭 ANSI 颜色。
- 超宽内容最多排到 100 列，避免在大屏上一行过长。
- 表格使用 `comfy-table` 的 UTF-8 边框，窄终端会自动压缩列宽。

## 项目结构

```
Makefile           构建、测试、检查、清理
src/
  main.rs          clap 命令定义与分发
  config.rs        URL、路径、浏览器指纹
  auth.rs          cookie 存取、二维码登录
  client.rs        知乎 API 客户端
  html.rs          HTML → 终端文本渲染器（含单元测试）
  display.rs       颜色、计数、表格、终端宽度
  util.rs          serde_json 取值助手
  commands/
    auth.rs        login/logout/status/whoami
    content.rs     search/hot/question/answers/answer/feed/feeds/topic
    user.rs        用户资料与列表
    interact.rs    赞同/关注/发布/删除/收藏/通知
```

运行测试：

```bash
cargo test
```

## 说明

- 本项目仅用于个人学习与命令行阅读，请遵守知乎的用户协议与 robots 规则，不要高频请求。
- 接口可能随知乎改版而变化；遇到 403/10003 时先确认登录态，必要时重新登录或降低请求频率。

## License

Apache-2.0，见 [LICENSE](LICENSE)。