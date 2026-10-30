//! HTML → 终端友好文本。
//!
//! 知乎接口返回的 `content` / `detail` / `excerpt` 都是 HTML。直接把标签删掉会把
//! `<p>`、`<br>`、`<li>` 等结构一起吃掉，正文变成一整段，在终端里几乎没法读。
//! 这里用一个小的两阶段渲染器：
//!
//! 1. 解析 HTML 为块级结构（段落、标题、列表、引用、代码块、图片、表格）；
//! 2. 按终端宽度做中英文混排换行，并保留列表 / 引用的缩进。
//!
//! 对中文按字断行，英文按单词断行，行首尽量避开 `，。！？` 等标点。

use ego_tree::NodeRef;
use scraper::{Html, Node};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// 把 HTML 渲染为适合终端的纯文本（保留段落、换行、列表、引用、代码块）。
pub fn render_html(html: &str, width: usize) -> String {
    let width = width.max(20);
    if html.trim().is_empty() {
        return String::new();
    }
    let doc = Html::parse_fragment(html);
    let blocks = parse_blocks(doc.tree.root());
    let mut renderer = Renderer::new(width);
    renderer.render_blocks(&blocks, 0, "");
    renderer.finish()
}

/// 把 HTML 压成单行文本，适合标题、摘要、表格单元格。
pub fn html_to_inline(html: &str) -> String {
    if html.trim().is_empty() {
        return String::new();
    }
    let doc = Html::parse_fragment(html);
    let mut out = String::new();
    for child in doc.tree.root().children() {
        collect_inline(child, &mut out);
    }
    clean_inline(&out)
}

// ── 块级结构 ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
enum Block {
    Para(String),
    Heading(String),
    List {
        ordered: bool,
        start: usize,
        items: Vec<Vec<Block>>,
    },
    Quote(Vec<Block>),
    Pre(String),
    Image(String),
    Rule,
}

fn is_block_tag(name: &str) -> bool {
    matches!(
        name,
        "p" | "div"
            | "section"
            | "article"
            | "header"
            | "footer"
            | "main"
            | "aside"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "ul"
            | "ol"
            | "blockquote"
            | "pre"
            | "figure"
            | "figcaption"
            | "table"
            | "hr"
            | "dl"
            | "dt"
            | "dd"
            | "details"
            | "summary"
            | "body"
            | "html"
    )
}

fn parse_blocks(root: NodeRef<'_, Node>) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut inline = String::new();
    for child in root.children() {
        if let Node::Element(el) = child.value() {
            if is_block_tag(el.name()) {
                flush_inline(&mut inline, &mut blocks);
                match el.name() {
                    "p" => {
                        let text = inline_of(child);
                        if !text.is_empty() {
                            blocks.push(Block::Para(text));
                        }
                    }
                    "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                        let text = inline_of(child);
                        if !text.is_empty() {
                            blocks.push(Block::Heading(text));
                        }
                    }
                    "ul" | "ol" => {
                        let start = if el.name() == "ol" {
                            el.attr("start")
                                .and_then(|s| s.parse::<usize>().ok())
                                .unwrap_or(1)
                        } else {
                            1
                        };
                        let mut items = Vec::new();
                        for li in child.children() {
                            if let Node::Element(li_el) = li.value() {
                                if li_el.name() == "li" {
                                    let mut item_blocks = Vec::new();
                                    let mut item_inline = String::new();
                                    for c in li.children() {
                                        if let Node::Element(ce) = c.value() {
                                            if is_block_tag(ce.name()) {
                                                flush_inline(&mut item_inline, &mut item_blocks);
                                                handle_block(c, ce.name(), &mut item_blocks);
                                                continue;
                                            }
                                        }
                                        collect_inline(c, &mut item_inline);
                                    }
                                    flush_inline(&mut item_inline, &mut item_blocks);
                                    if item_blocks.is_empty() {
                                        item_blocks.push(Block::Para(String::new()));
                                    }
                                    items.push(item_blocks);
                                }
                            }
                        }
                        if !items.is_empty() {
                            blocks.push(Block::List {
                                ordered: el.name() == "ol",
                                start,
                                items,
                            });
                        }
                    }
                    "blockquote" => {
                        let inner = parse_blocks(child);
                        if !inner.is_empty() {
                            blocks.push(Block::Quote(inner));
                        }
                    }
                    "pre" => {
                        let raw = raw_text(child);
                        let raw = raw.trim_matches('\n').to_string();
                        if !raw.trim().is_empty() {
                            blocks.push(Block::Pre(raw));
                        }
                    }
                    "figure" => {
                        let mut caption = String::new();
                        let mut has_image = false;
                        let mut has_video = false;
                        for desc in child.descendants() {
                            if let Node::Element(de) = desc.value() {
                                match de.name() {
                                    "img" => has_image = true,
                                    "video" => has_video = true,
                                    "figcaption" => {
                                        caption = inline_of(desc);
                                    }
                                    _ => {}
                                }
                            }
                        }
                        if has_image || has_video {
                            if caption.is_empty() {
                                caption = if has_image {
                                    "图片".into()
                                } else {
                                    "视频".into()
                                };
                            }
                            blocks.push(Block::Image(caption));
                        } else if !caption.is_empty() {
                            blocks.push(Block::Para(caption));
                        }
                    }
                    "figcaption" => {
                        let text = inline_of(child);
                        if !text.is_empty() {
                            blocks.push(Block::Para(text));
                        }
                    }
                    "hr" => blocks.push(Block::Rule),
                    "table" => {
                        let rows = parse_table(child);
                        if !rows.is_empty() {
                            blocks.push(Block::Pre(rows.join("\n")));
                        }
                    }
                    // div / section / article 等容器：递归拆分
                    _ => {
                        let inner = parse_blocks(child);
                        blocks.extend(inner);
                    }
                }
            } else {
                collect_inline(child, &mut inline);
            }
        } else {
            collect_inline(child, &mut inline);
        }
    }
    flush_inline(&mut inline, &mut blocks);
    blocks
}

fn handle_block(node: NodeRef<'_, Node>, name: &str, blocks: &mut Vec<Block>) {
    match name {
        "p" => {
            let text = inline_of(node);
            if !text.is_empty() {
                blocks.push(Block::Para(text));
            }
        }
        "ul" | "ol" => {
            let el = node.value().as_element().unwrap();
            let start = if name == "ol" {
                el.attr("start")
                    .and_then(|s| s.parse::<usize>().ok())
                    .unwrap_or(1)
            } else {
                1
            };
            let mut items = Vec::new();
            for li in node.children() {
                if let Node::Element(li_el) = li.value() {
                    if li_el.name() == "li" {
                        let mut item_blocks = Vec::new();
                        let mut item_inline = String::new();
                        for c in li.children() {
                            if let Node::Element(ce) = c.value() {
                                if is_block_tag(ce.name()) {
                                    flush_inline(&mut item_inline, &mut item_blocks);
                                    handle_block(c, ce.name(), &mut item_blocks);
                                    continue;
                                }
                            }
                            collect_inline(c, &mut item_inline);
                        }
                        flush_inline(&mut item_inline, &mut item_blocks);
                        if item_blocks.is_empty() {
                            item_blocks.push(Block::Para(String::new()));
                        }
                        items.push(item_blocks);
                    }
                }
            }
            if !items.is_empty() {
                blocks.push(Block::List {
                    ordered: name == "ol",
                    start,
                    items,
                });
            }
        }
        "blockquote" => {
            let inner = parse_blocks(node);
            if !inner.is_empty() {
                blocks.push(Block::Quote(inner));
            }
        }
        "pre" => {
            let raw = raw_text(node);
            let raw = raw.trim_matches('\n').to_string();
            if !raw.trim().is_empty() {
                blocks.push(Block::Pre(raw));
            }
        }
        "hr" => blocks.push(Block::Rule),
        "figure" => {
            let inner = parse_blocks(node);
            blocks.extend(inner);
        }
        _ => {
            if let Some(el) = scraper::ElementRef::wrap(node) {
                blocks.extend(parse_blocks(*el));
            }
        }
    }
}

fn flush_inline(inline: &mut String, blocks: &mut Vec<Block>) {
    let text = clean_inline(inline);
    inline.clear();
    if !text.is_empty() {
        blocks.push(Block::Para(text));
    }
}

fn inline_of(node: NodeRef<'_, Node>) -> String {
    let mut out = String::new();
    for child in node.children() {
        collect_inline(child, &mut out);
    }
    clean_inline(&out)
}

fn parse_table(node: NodeRef<'_, Node>) -> Vec<String> {
    let mut rows = Vec::new();
    for desc in node.descendants() {
        if let Node::Element(el) = desc.value() {
            if el.name() == "tr" {
                let mut cells = Vec::new();
                for cell in desc.children() {
                    if let Node::Element(ce) = cell.value() {
                        if matches!(ce.name(), "td" | "th") {
                            let text = inline_of(cell);
                            cells.push(text);
                        }
                    }
                }
                if !cells.is_empty() {
                    rows.push(cells.join("  │  "));
                }
            }
        }
    }
    rows
}

fn raw_text(node: NodeRef<'_, Node>) -> String {
    let mut out = String::new();
    fn walk(node: NodeRef<'_, Node>, out: &mut String) {
        match node.value() {
            Node::Text(t) => out.push_str(&t.text),
            Node::Element(el) if el.name() == "br" => out.push('\n'),
            _ => {
                for c in node.children() {
                    walk(c, out);
                }
            }
        }
    }
    walk(node, &mut out);
    out
}

// ── 行内文本 ──────────────────────────────────────────────────────────────────

fn collect_inline(node: NodeRef<'_, Node>, out: &mut String) {
    match node.value() {
        Node::Text(t) => push_text(&t.text, out),
        Node::Element(el) => match el.name() {
            "br" => {
                trim_trailing_spaces(out);
                if !out.ends_with('\n') && !out.is_empty() {
                    out.push('\n');
                }
            }
            "img" => {
                let alt = el.attr("alt").unwrap_or("").trim();
                let marker = if alt.is_empty() {
                    "[图片]".to_string()
                } else {
                    format!("[图片: {}]", alt)
                };
                push_atom(&marker, out);
            }
            "video" => push_atom("[视频]", out),
            "script" | "style" | "noscript" | "svg" | "head" => {}
            "a" => {
                let mut inner = String::new();
                for c in node.children() {
                    collect_inline(c, &mut inner);
                }
                let text = clean_inline(&inner);
                if text.is_empty() {
                    if let Some(href) = el.attr("href") {
                        push_atom(href, out);
                    }
                } else {
                    out.push_str(&text);
                }
            }
            "code" => {
                let mut inner = String::new();
                for c in node.children() {
                    collect_inline(c, &mut inner);
                }
                let text = inner.trim();
                if !text.is_empty() {
                    out.push('`');
                    out.push_str(text);
                    out.push('`');
                }
            }
            _ => {
                for c in node.children() {
                    collect_inline(c, out);
                }
            }
        },
        _ => {
            for c in node.children() {
                collect_inline(c, out);
            }
        }
    }
}

fn push_text(text: &str, out: &mut String) {
    for ch in text.chars() {
        if ch.is_whitespace() {
            if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
                out.push(' ');
            }
        } else {
            out.push(ch);
        }
    }
}

fn push_atom(atom: &str, out: &mut String) {
    if !out.is_empty() && !out.ends_with(' ') && !out.ends_with('\n') {
        out.push(' ');
    }
    out.push_str(atom);
    out.push(' ');
}

fn trim_trailing_spaces(s: &mut String) {
    while s.ends_with(' ') || s.ends_with('\t') {
        s.pop();
    }
}

fn clean_inline(s: &str) -> String {
    let mut lines: Vec<String> = s
        .split('\n')
        .map(|line| {
            line.split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .trim()
                .to_string()
        })
        .collect();
    // 去掉首尾空行
    while lines.first().is_some_and(|l| l.is_empty()) {
        lines.remove(0);
    }
    while lines.last().is_some_and(|l| l.is_empty()) {
        lines.pop();
    }
    lines.join("\n")
}

// ── 版面 ──────────────────────────────────────────────────────────────────────

struct Renderer {
    width: usize,
    out: String,
}

impl Renderer {
    fn new(width: usize) -> Self {
        Self {
            width: width.max(20),
            out: String::new(),
        }
    }

    fn finish(mut self) -> String {
        while self.out.ends_with('\n') {
            self.out.pop();
        }
        self.out
    }

    /// 确保块之间恰好有一个空行。
    fn blank(&mut self) {
        if self.out.is_empty() {
            return;
        }
        while self.out.ends_with('\n') {
            self.out.pop();
        }
        self.out.push_str("\n\n");
    }

    fn base(&self, indent: usize, prefix: &str) -> String {
        format!("{}{}", " ".repeat(indent), prefix)
    }

    fn render_blocks(&mut self, blocks: &[Block], indent: usize, prefix: &str) {
        for block in blocks {
            match block {
                Block::Para(text) => {
                    self.blank();
                    self.write_wrapped(text, indent, prefix);
                }
                Block::Heading(text) => {
                    self.blank();
                    self.write_heading(text, indent, prefix);
                }
                Block::Quote(inner) => {
                    self.blank();
                    let new_prefix = format!("{}│ ", prefix);
                    self.render_blocks(inner, indent, &new_prefix);
                }
                Block::List {
                    ordered,
                    start,
                    items,
                } => {
                    self.blank();
                    self.render_list(*ordered, *start, items, indent, prefix);
                }
                Block::Pre(text) => {
                    self.blank();
                    self.write_pre(text, indent, prefix);
                }
                Block::Image(caption) => {
                    self.blank();
                    let base = self.base(indent, prefix);
                    let label = if caption == "图片" || caption == "视频" {
                        format!("[{}]", caption)
                    } else {
                        format!("[图片] {}", caption)
                    };
                    let avail = self.width.saturating_sub(base.width()).max(12);
                    for line in wrap_line(&label, avail) {
                        self.out.push_str(&base);
                        self.out.push_str(&line);
                        self.out.push('\n');
                    }
                }
                Block::Rule => {
                    self.blank();
                    let base = self.base(indent, prefix);
                    let len = self.width.saturating_sub(base.width()).clamp(6, 48);
                    self.out.push_str(&base);
                    self.out.push_str(&"─".repeat(len));
                    self.out.push('\n');
                }
            }
        }
    }

    fn write_wrapped(&mut self, text: &str, indent: usize, prefix: &str) {
        let base = self.base(indent, prefix);
        let avail = self.width.saturating_sub(base.width()).max(12);
        for logical in text.split('\n') {
            let logical = logical.trim();
            if logical.is_empty() {
                continue;
            }
            for line in wrap_line(logical, avail) {
                self.out.push_str(&base);
                self.out.push_str(&line);
                self.out.push('\n');
            }
        }
    }

    fn write_heading(&mut self, text: &str, indent: usize, prefix: &str) {
        let base = self.base(indent, prefix);
        let avail = self.width.saturating_sub(base.width() + 2).max(12);
        let mut lines = Vec::new();
        for logical in text.split('\n') {
            let logical = logical.trim();
            if logical.is_empty() {
                continue;
            }
            lines.extend(wrap_line(logical, avail));
        }
        for (i, line) in lines.iter().enumerate() {
            self.out.push_str(&base);
            if i == 0 {
                self.out.push_str("◆ ");
            } else {
                self.out.push_str("  ");
            }
            self.out.push_str(line);
            self.out.push('\n');
        }
        if !lines.is_empty() {
            let underline = lines
                .iter()
                .map(|l| l.width())
                .max()
                .unwrap_or(2)
                .min(avail)
                .max(2);
            self.out.push_str(&base);
            self.out.push_str(&"─".repeat(underline));
            self.out.push('\n');
        }
    }

    fn write_pre(&mut self, text: &str, indent: usize, prefix: &str) {
        let base = self.base(indent, prefix);
        let code_base = format!("{}  ", base);
        let avail = self.width.saturating_sub(code_base.width()).max(12);
        for logical in text.split('\n') {
            let logical = logical.replace('\t', "    ");
            let logical = logical.trim_end();
            if logical.is_empty() {
                self.out.push('\n');
                continue;
            }
            if logical.width() <= avail {
                self.out.push_str(&code_base);
                self.out.push_str(logical);
                self.out.push('\n');
                continue;
            }
            // 代码块按字符硬换行，保留缩进和连续空格
            let mut line = String::new();
            let mut line_width = 0usize;
            for ch in logical.chars() {
                let w = ch.width().unwrap_or(0);
                if line_width + w > avail && !line.is_empty() {
                    self.out.push_str(&code_base);
                    self.out.push_str(&line);
                    self.out.push('\n');
                    line.clear();
                    line_width = 0;
                }
                line.push(ch);
                line_width += w;
            }
            if !line.is_empty() {
                self.out.push_str(&code_base);
                self.out.push_str(&line);
                self.out.push('\n');
            }
        }
    }

    fn render_list(
        &mut self,
        ordered: bool,
        start: usize,
        items: &[Vec<Block>],
        indent: usize,
        prefix: &str,
    ) {
        let base = self.base(indent, prefix);
        for (i, item) in items.iter().enumerate() {
            let marker = if ordered {
                format!("{}. ", start + i)
            } else {
                "• ".to_string()
            };
            let marker_width = marker.width();
            let avail = self
                .width
                .saturating_sub(base.width() + marker_width)
                .max(12);
            let mut sub = Renderer::new(avail);
            sub.render_blocks(item, 0, "");
            let body = sub.finish();
            if body.is_empty() {
                continue;
            }
            for (line_idx, line) in body.lines().enumerate() {
                if line.is_empty() {
                    self.out.push('\n');
                    continue;
                }
                self.out.push_str(&base);
                if line_idx == 0 {
                    self.out.push_str(&marker);
                } else {
                    self.out.push_str(&" ".repeat(marker_width));
                }
                self.out.push_str(line);
                self.out.push('\n');
            }
        }
    }
}

// ── 换行 ──────────────────────────────────────────────────────────────────────

/// 中英文混排换行：英文按单词、中文按字断行，行首避开收尾标点。
pub fn wrap_line(line: &str, width: usize) -> Vec<String> {
    let width = width.max(1);
    let tokens = tokenize(line);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut current_width = 0usize;

    for (space_before, token) in tokens {
        let token_width = token.width();
        let sep = usize::from(space_before && !current.is_empty());
        if current_width + sep + token_width <= width {
            if sep == 1 {
                current.push(' ');
            }
            current.push_str(&token);
            current_width += sep + token_width;
            continue;
        }
        if !current.is_empty() {
            lines.push(std::mem::take(&mut current));
            current_width = 0;
        }
        if token_width <= width {
            current.push_str(&token);
            current_width = token_width;
        } else {
            // 超长 token（长 URL、连续英文）按字符硬断
            for ch in token.chars() {
                let w = ch.width().unwrap_or(0);
                if current_width + w > width && !current.is_empty() {
                    lines.push(std::mem::take(&mut current));
                    current_width = 0;
                }
                current.push(ch);
                current_width += w;
            }
        }
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}

fn tokenize(s: &str) -> Vec<(bool, String)> {
    let mut tokens: Vec<(bool, String)> = Vec::new();
    let mut word = String::new();
    let mut word_space = false;
    let mut pending_space = false;

    let flush_word = |word: &mut String, word_space: bool, tokens: &mut Vec<(bool, String)>| {
        if !word.is_empty() {
            tokens.push((word_space, std::mem::take(word)));
        }
    };

    for ch in s.chars() {
        if ch.is_whitespace() {
            flush_word(&mut word, word_space, &mut tokens);
            pending_space = true;
            continue;
        }
        // 收尾标点粘在前一个 token 上，避免出现在行首
        if is_closing_punct(ch) && (!word.is_empty() || !tokens.is_empty()) {
            if !word.is_empty() {
                word.push(ch);
            } else if let Some(last) = tokens.last_mut() {
                last.1.push(ch);
            }
            pending_space = false;
            continue;
        }
        if is_cjk(ch) {
            flush_word(&mut word, word_space, &mut tokens);
            tokens.push((pending_space, ch.to_string()));
            pending_space = false;
        } else {
            if word.is_empty() {
                word_space = pending_space;
            }
            word.push(ch);
            pending_space = false;
        }
    }
    flush_word(&mut word, word_space, &mut tokens);
    tokens
}

fn is_cjk(ch: char) -> bool {
    matches!(ch as u32,
        0x1100..=0x11FF
        | 0x2E80..=0x303E
        | 0x3041..=0x33FF
        | 0x3400..=0x4DBF
        | 0x4E00..=0x9FFF
        | 0xA000..=0xA4CF
        | 0xAC00..=0xD7AF
        | 0xF900..=0xFAFF
        | 0xFE30..=0xFE4F
        | 0xFF00..=0xFF60
        | 0xFFE0..=0xFFE6
        | 0x20000..=0x3FFFD
    )
}

fn is_closing_punct(ch: char) -> bool {
    matches!(
        ch,
        '，' | '。'
            | '！'
            | '？'
            | '；'
            | '：'
            | '、'
            | '）'
            | '】'
            | '》'
            | '」'
            | '』'
            | '”'
            | '’'
            | '…'
            | ','
            | '.'
            | '!'
            | '?'
            | ';'
            | ':'
            | ')'
            | ']'
            | '}'
            | '%'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paragraphs_are_preserved() {
        let html = "<p>第一段内容。</p><p>第二段内容。</p>";
        let out = render_html(html, 40);
        assert!(out.contains("第一段内容。\n\n第二段内容。"), "{out}");
    }

    #[test]
    fn br_creates_newline() {
        let out = render_html("<p>第一行<br>第二行</p>", 40);
        assert_eq!(out, "第一行\n第二行");
    }

    #[test]
    fn list_is_rendered() {
        let out = render_html("<ul><li>一</li><li>二</li></ul>", 40);
        assert!(out.contains("• 一"), "{out}");
        assert!(out.contains("• 二"), "{out}");
    }

    #[test]
    fn cjk_wrap_does_not_overflow() {
        let text = "这是一段很长的中文内容，用来测试终端换行是否正常工作，不应该超过指定宽度。";
        for line in wrap_line(text, 16) {
            assert!(line.width() <= 16, "{line} width={}", line.width());
        }
    }

    #[test]
    fn renders_rich_html() {
        let html = r#"
          <h2>标题</h2>
          <p>第一段，包含 <b>加粗</b>、<a href="https://example.com">链接</a> 和 <code>inline_code()</code>。</p>
          <p>第二段<br>换行后的内容。</p>
          <ul>
            <li>项目一</li>
            <li>项目二，内容比较长，需要自动换行显示，不能把整个段落挤成一行。</li>
          </ul>
          <blockquote><p>这是一段引用文字，应该带有左侧竖线。</p></blockquote>
          <pre><code>fn main() {
    println!("hello");
}</code></pre>
          <figure><img src="x.jpg" alt=""><figcaption>一张图片的说明</figcaption></figure>
        "#;
        let out = render_html(html, 40);
        println!("\n---- rendered ----\n{out}\n------------------");
        assert!(out.contains("第一段"));
        assert!(out.contains("• 项目一"));
        assert!(out.contains("│ 这是一段引用文字"));
        assert!(out.contains("fn main()"));
        assert!(out.contains("[图片] 一张图片的说明"));
    }

    #[test]
    fn rendered_lines_fit_width() {
        let html = r#"
          <h3>一个比较长的标题，用来检查标题换行是否会溢出终端宽度</h3>
          <p>这是一段很长的中文正文，包含 English words and a long URL
          https://example.com/some/very/long/path/that/should/be/hard/wrapped
          以及标点符号，最后应该每一行都不超过指定宽度。</p>
          <blockquote><p>引用里的内容也应该按同样的宽度换行，并且保留竖线前缀。</p></blockquote>
          <ul><li>列表项一，内容长一点，长到需要换行才能完整显示。</li></ul>
          <pre><code>let s = "a very long code line that should be hard wrapped in the terminal";</code></pre>
        "#;
        let out = render_html(html, 30);
        for line in out.lines() {
            assert!(
                line.width() <= 30,
                "line too wide ({}): {line:?}",
                line.width()
            );
        }
    }

    #[test]
    fn inline_text_collapses_whitespace() {
        assert_eq!(
            html_to_inline("<p>hello   <b>world</b>\n  again</p>"),
            "hello world again"
        );
    }
}
