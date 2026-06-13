//! Minimal Markdown -> HTML renderer with no external dependencies.
//!
//! Implements the subset of (GitHub-flavored) Markdown used by the OxideOS
//! docs: ATX headings, paragraphs, fenced code blocks, blockquotes, ordered
//! and unordered (nested) lists, pipe tables, horizontal rules, raw HTML
//! passthrough lines, and the common inline spans (code, emphasis, strong,
//! strikethrough, links, images, autolinks).

use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq)]
enum Align {
    None,
    Left,
    Center,
    Right,
}

enum Block {
    Heading(u8, String),
    Paragraph(String),
    Code(Option<String>, String),
    Html(String),
    Rule,
    Quote(Vec<Block>),
    List {
        ordered: bool,
        start: u64,
        items: Vec<Vec<Block>>,
    },
    Table {
        head: Vec<String>,
        align: Vec<Align>,
        rows: Vec<Vec<String>>,
    },
}

/// Render a full Markdown document to an HTML fragment.
pub fn to_html(src: &str) -> String {
    let lines: Vec<String> = src.lines().map(|l| l.to_string()).collect();
    let blocks = parse_blocks(&lines);
    let mut out = String::new();
    let mut slugs = HashMap::new();
    render_blocks(&blocks, &mut out, &mut slugs);
    out
}

/// Escape plain text for safe inclusion in HTML.
pub fn escape_text(s: &str) -> String {
    escape_html(s)
}

/// Extract the text of the first level-1/2 heading, if any (used as a page title).
pub fn first_heading(src: &str) -> Option<String> {
    for line in src.lines() {
        if let Some((_, text)) = heading(line) {
            if !text.is_empty() {
                return Some(text.to_string());
            }
        }
    }
    None
}

// ───────────────────────── block-level parsing ─────────────────────────

fn leading_spaces(line: &str) -> usize {
    line.chars().take_while(|&c| c == ' ').count()
}

fn dedent(line: &str, col: usize) -> String {
    let mut removed = 0;
    let mut leading = true;
    let mut out = String::new();
    for ch in line.chars() {
        if leading && removed < col && ch == ' ' {
            removed += 1;
            continue;
        }
        leading = false;
        out.push(ch);
    }
    out
}

/// Remove exactly `col` leading characters (used to strip a list marker plus its
/// indentation, which may include non-space characters like `-` or `1.`).
fn strip_cols(line: &str, col: usize) -> String {
    let chars: Vec<char> = line.chars().collect();
    if chars.len() <= col {
        return String::new();
    }
    chars[col..].iter().collect()
}

fn heading(line: &str) -> Option<(u8, &str)> {
    if leading_spaces(line) > 3 {
        return None;
    }
    let trimmed = line.trim_start();
    let hashes = trimmed.chars().take_while(|&c| c == '#').count();
    if hashes == 0 || hashes > 6 {
        return None;
    }
    let rest = &trimmed[hashes..];
    if !rest.is_empty() && !rest.starts_with(' ') && !rest.starts_with('\t') {
        return None;
    }
    let text = rest.trim().trim_end_matches('#').trim_end();
    Some((hashes as u8, text))
}

fn is_hr(line: &str) -> bool {
    let t = line.trim();
    if t.len() < 3 {
        return false;
    }
    let c = t.chars().next().unwrap();
    if c != '-' && c != '*' && c != '_' {
        return false;
    }
    t.chars().filter(|&ch| ch != ' ').count() >= 3 && t.chars().all(|ch| ch == c || ch == ' ')
}

fn fence(line: &str) -> Option<(char, usize, String)> {
    let indent = leading_spaces(line);
    if indent > 3 {
        return None;
    }
    let t = &line[indent..];
    let c = t.chars().next()?;
    if c != '`' && c != '~' {
        return None;
    }
    let n = t.chars().take_while(|&ch| ch == c).count();
    if n < 3 {
        return None;
    }
    Some((c, n, t[n..].trim().to_string()))
}

fn is_quote(line: &str) -> bool {
    let indent = leading_spaces(line);
    indent <= 3 && line[indent..].starts_with('>')
}

fn quote_strip(line: &str) -> String {
    let indent = leading_spaces(line);
    let rest = &line[indent..][1..];
    if rest.starts_with(' ') {
        rest[1..].to_string()
    } else {
        rest.to_string()
    }
}

#[derive(Clone, Copy)]
enum Marker {
    Bullet(char),
    Ordered(u64, char),
}

/// Returns (marker, marker indent column, content start column).
fn list_marker(line: &str) -> Option<(Marker, usize, usize)> {
    let indent = leading_spaces(line);
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let mut chars = rest.chars();
    let c = chars.next()?;
    if c == '-' || c == '*' || c == '+' {
        let after = &rest[1..];
        if after.is_empty() || after.starts_with(' ') || after.starts_with('\t') {
            let spaces = after.chars().take_while(|&ch| ch == ' ' || ch == '\t').count();
            let spaces = if after.is_empty() { 1 } else { spaces.max(1).min(4) };
            return Some((Marker::Bullet(c), indent, indent + 1 + spaces));
        }
        return None;
    }
    if !c.is_ascii_digit() {
        return None;
    }
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    if digits.len() > 9 {
        return None;
    }
    let after_digits = &rest[digits.len()..];
    let delim = after_digits.chars().next()?;
    if delim != '.' && delim != ')' {
        return None;
    }
    let after = &after_digits[1..];
    if !(after.is_empty() || after.starts_with(' ') || after.starts_with('\t')) {
        return None;
    }
    let num: u64 = digits.parse().ok()?;
    let marker_len = digits.len() + 1;
    let spaces = after.chars().take_while(|&ch| ch == ' ' || ch == '\t').count();
    let spaces = if after.is_empty() { 1 } else { spaces.max(1).min(4) };
    Some((Marker::Ordered(num, delim), indent, indent + marker_len + spaces))
}

fn table_separator(line: &str) -> Option<Vec<Align>> {
    let t = line.trim();
    let t = t.trim_start_matches('|').trim_end_matches('|');
    if t.is_empty() {
        return None;
    }
    let mut aligns = Vec::new();
    for cell in t.split('|') {
        let cell = cell.trim();
        if cell.is_empty() {
            return None;
        }
        let left = cell.starts_with(':');
        let right = cell.ends_with(':');
        let dashes = cell.trim_matches(':');
        if dashes.is_empty() || !dashes.chars().all(|c| c == '-') {
            return None;
        }
        aligns.push(match (left, right) {
            (true, true) => Align::Center,
            (true, false) => Align::Left,
            (false, true) => Align::Right,
            (false, false) => Align::None,
        });
    }
    Some(aligns)
}

fn split_row(line: &str) -> Vec<String> {
    let t = line.trim();
    let t = t.strip_prefix('|').unwrap_or(t);
    let t = t.strip_suffix('|').unwrap_or(t);
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(&next) = chars.peek() {
                cur.push(c);
                cur.push(next);
                chars.next();
                continue;
            }
        }
        if c == '|' {
            cells.push(cur.trim().to_string());
            cur = String::new();
        } else {
            cur.push(c);
        }
    }
    cells.push(cur.trim().to_string());
    cells
}

fn is_html_line(line: &str) -> bool {
    let t = line.trim();
    if !t.starts_with('<') {
        return false;
    }
    if t.starts_with("<!--") {
        return true;
    }
    let rest = t.strip_prefix('<').unwrap();
    let rest = rest.strip_prefix('/').unwrap_or(rest);
    matches!(rest.chars().next(), Some(c) if c.is_ascii_alphabetic())
}

fn starts_block(line: &str) -> bool {
    heading(line).is_some()
        || is_hr(line)
        || fence(line).is_some()
        || is_quote(line)
        || list_marker(line).is_some()
        || is_html_line(line)
}

fn parse_blocks(lines: &[String]) -> Vec<Block> {
    let mut blocks = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i].as_str();
        if line.trim().is_empty() {
            i += 1;
            continue;
        }

        // Fenced code block
        if let Some((fc, flen, info)) = fence(line) {
            let base_indent = leading_spaces(line);
            let mut code_lines = Vec::new();
            i += 1;
            while i < lines.len() {
                if let Some((fc2, flen2, info2)) = fence(&lines[i]) {
                    if fc2 == fc && flen2 >= flen && info2.is_empty() {
                        i += 1;
                        break;
                    }
                }
                code_lines.push(dedent(&lines[i], base_indent));
                i += 1;
            }
            let lang = info.split_whitespace().next().map(|s| s.to_string());
            blocks.push(Block::Code(lang, code_lines.join("\n")));
            continue;
        }

        // Heading
        if let Some((level, text)) = heading(line) {
            blocks.push(Block::Heading(level, text.to_string()));
            i += 1;
            continue;
        }

        // Horizontal rule
        if is_hr(line) {
            blocks.push(Block::Rule);
            i += 1;
            continue;
        }

        // Blockquote
        if is_quote(line) {
            let mut quote_lines = Vec::new();
            while i < lines.len() && is_quote(&lines[i]) {
                quote_lines.push(quote_strip(&lines[i]));
                i += 1;
            }
            blocks.push(Block::Quote(parse_blocks(&quote_lines)));
            continue;
        }

        // Lists
        if let Some((marker, indent, _)) = list_marker(line) {
            let ordered = matches!(marker, Marker::Ordered(_, _));
            let start = match marker {
                Marker::Ordered(n, _) => n,
                _ => 1,
            };
            let marker_char = match marker {
                Marker::Bullet(c) => c,
                Marker::Ordered(_, d) => d,
            };
            let mut items: Vec<Vec<Block>> = Vec::new();

            loop {
                if i >= lines.len() {
                    break;
                }
                if lines[i].trim().is_empty() {
                    break;
                }
                let Some((m, ind, cc)) = list_marker(&lines[i]) else {
                    break;
                };
                let same_kind = match (m, marker) {
                    (Marker::Bullet(a), Marker::Bullet(_)) => a == marker_char,
                    (Marker::Ordered(_, a), Marker::Ordered(_, _)) => a == marker_char,
                    _ => false,
                };
                if ind != indent || !same_kind {
                    break;
                }

                let mut item_lines = vec![strip_cols(&lines[i], cc)];
                i += 1;
                loop {
                    if i >= lines.len() {
                        break;
                    }
                    if lines[i].trim().is_empty() {
                        let mut j = i + 1;
                        while j < lines.len() && lines[j].trim().is_empty() {
                            j += 1;
                        }
                        if j < lines.len() && leading_spaces(&lines[j]) >= cc {
                            item_lines.push(String::new());
                            i += 1;
                            continue;
                        } else {
                            break;
                        }
                    }
                    if leading_spaces(&lines[i]) >= cc {
                        item_lines.push(strip_cols(&lines[i], cc));
                        i += 1;
                    } else {
                        break;
                    }
                }
                items.push(parse_blocks(&item_lines));
            }

            blocks.push(Block::List { ordered, start, items });
            continue;
        }

        // Tables
        if line.contains('|') {
            if let Some(next) = lines.get(i + 1) {
                if let Some(aligns) = table_separator(next) {
                    let head = split_row(line);
                    let mut rows = Vec::new();
                    let mut j = i + 2;
                    while j < lines.len() && lines[j].contains('|') && !lines[j].trim().is_empty() {
                        rows.push(split_row(&lines[j]));
                        j += 1;
                    }
                    blocks.push(Block::Table { head, align: aligns, rows });
                    i = j;
                    continue;
                }
            }
        }

        // Raw HTML passthrough
        if is_html_line(line) {
            let mut html_lines = vec![line.to_string()];
            i += 1;
            loop {
                if i >= lines.len() {
                    break;
                }
                if lines[i].trim().is_empty() {
                    if i + 1 < lines.len() && is_html_line(&lines[i + 1]) {
                        html_lines.push(String::new());
                        i += 1;
                        continue;
                    } else {
                        break;
                    }
                }
                if is_html_line(&lines[i]) {
                    html_lines.push(lines[i].clone());
                    i += 1;
                } else {
                    break;
                }
            }
            blocks.push(Block::Html(html_lines.join("\n")));
            continue;
        }

        // Paragraph
        let mut para_lines = vec![line.to_string()];
        i += 1;
        while i < lines.len() {
            let l = lines[i].as_str();
            if l.trim().is_empty() || starts_block(l) {
                break;
            }
            para_lines.push(l.to_string());
            i += 1;
        }
        blocks.push(Block::Paragraph(para_lines.join("\n")));
    }
    blocks
}

// ───────────────────────── rendering ─────────────────────────

fn align_attr(a: Align) -> &'static str {
    match a {
        Align::None => "",
        Align::Left => " style=\"text-align:left\"",
        Align::Center => " style=\"text-align:center\"",
        Align::Right => " style=\"text-align:right\"",
    }
}

fn slugify(text: &str, slugs: &mut HashMap<String, u32>) -> String {
    let plain = strip_inline_markup(text);
    let mut slug = String::new();
    let mut last_dash = false;
    for c in plain.chars() {
        if c.is_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if (c == ' ' || c == '-' || c == '_') && !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-').to_string();
    let slug = if slug.is_empty() { "section".to_string() } else { slug };
    let n = slugs.entry(slug.clone()).or_insert(0);
    let result = if *n == 0 { slug.clone() } else { format!("{}-{}", slug, n) };
    *n += 1;
    result
}

/// Remove inline markup characters so heading text can be turned into an anchor slug.
fn strip_inline_markup(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '*' | '_' | '`' | '#' | '~'))
        .collect()
}

fn render_blocks(blocks: &[Block], out: &mut String, slugs: &mut HashMap<String, u32>) {
    for b in blocks {
        render_block(b, out, slugs);
    }
}

fn render_block(b: &Block, out: &mut String, slugs: &mut HashMap<String, u32>) {
    match b {
        Block::Heading(level, text) => {
            let id = slugify(text, slugs);
            out.push_str(&format!("<h{level} id=\"{id}\">{inner}</h{level}>\n", level = level, id = id, inner = render_inline(text)));
        }
        Block::Paragraph(text) => {
            out.push_str("<p>");
            out.push_str(&render_inline_multiline(text));
            out.push_str("</p>\n");
        }
        Block::Code(lang, code) => {
            match lang {
                Some(l) => out.push_str(&format!("<pre><code class=\"language-{}\">", escape_attr(l))),
                None => out.push_str("<pre><code>"),
            }
            out.push_str(&escape_html(code));
            out.push_str("</code></pre>\n");
        }
        Block::Html(html) => {
            out.push_str(html);
            out.push('\n');
        }
        Block::Rule => out.push_str("<hr>\n"),
        Block::Quote(inner) => {
            out.push_str("<blockquote>\n");
            render_blocks(inner, out, slugs);
            out.push_str("</blockquote>\n");
        }
        Block::List { ordered, start, items } => {
            let tag = if *ordered { "ol" } else { "ul" };
            if *ordered && *start != 1 {
                out.push_str(&format!("<{} start=\"{}\">\n", tag, start));
            } else {
                out.push_str(&format!("<{}>\n", tag));
            }
            for item in items {
                out.push_str("<li>");
                render_list_item(item, out, slugs);
                out.push_str("</li>\n");
            }
            out.push_str(&format!("</{}>\n", tag));
        }
        Block::Table { head, align, rows } => {
            out.push_str("<table>\n<thead>\n<tr>\n");
            for (idx, h) in head.iter().enumerate() {
                let a = align.get(idx).copied().unwrap_or(Align::None);
                out.push_str(&format!("<th{}>{}</th>\n", align_attr(a), render_inline(h)));
            }
            out.push_str("</tr>\n</thead>\n<tbody>\n");
            for row in rows {
                out.push_str("<tr>\n");
                for (idx, cell) in row.iter().enumerate() {
                    let a = align.get(idx).copied().unwrap_or(Align::None);
                    out.push_str(&format!("<td{}>{}</td>\n", align_attr(a), render_inline(cell)));
                }
                out.push_str("</tr>\n");
            }
            out.push_str("</tbody>\n</table>\n");
        }
    }
}

fn render_list_item(blocks: &[Block], out: &mut String, slugs: &mut HashMap<String, u32>) {
    for (idx, b) in blocks.iter().enumerate() {
        if idx == 0 {
            if let Block::Paragraph(text) = b {
                out.push_str(&render_inline_multiline(text));
                continue;
            }
        }
        render_block(b, out, slugs);
    }
}

fn render_inline_multiline(text: &str) -> String {
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = String::new();
    for (idx, line) in lines.iter().enumerate() {
        let hard_break = line.ends_with("  ") || line.ends_with('\\');
        let trimmed = line.trim_end_matches(' ').trim_end_matches('\\');
        out.push_str(&render_inline(trimmed));
        if idx + 1 < lines.len() {
            if hard_break {
                out.push_str("<br>\n");
            } else {
                out.push('\n');
            }
        }
    }
    out
}

// ───────────────────────── inline parsing ─────────────────────────

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            _ => out.push(c),
        }
    }
    out
}

fn escape_attr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

fn push_escaped(out: &mut String, c: char) {
    match c {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        _ => out.push(c),
    }
}

fn is_escapable(c: char) -> bool {
    matches!(
        c,
        '\\' | '`' | '*' | '_' | '{' | '}' | '[' | ']' | '(' | ')' | '#' | '+' | '-' | '.' | '!' | '<' | '>' | '"' | '\'' | '|' | '~'
    )
}

pub fn render_inline(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    render_span(&chars, 0, chars.len())
}

fn render_span(chars: &[char], start: usize, end: usize) -> String {
    let mut out = String::new();
    let mut i = start;
    while i < end {
        let c = chars[i];
        match c {
            '\\' if i + 1 < end && is_escapable(chars[i + 1]) => {
                push_escaped(&mut out, chars[i + 1]);
                i += 2;
            }
            '`' => {
                let mut n = 0;
                while i < end && chars[i] == '`' {
                    n += 1;
                    i += 1;
                }
                if let Some(close) = find_backtick_close(chars, i, end, n) {
                    let mut code: Vec<char> = chars[i..close].to_vec();
                    if code.first() == Some(&' ') && code.last() == Some(&' ') && code.iter().any(|c| *c != ' ') {
                        code.remove(0);
                        code.pop();
                    }
                    let code_str: String = code.into_iter().collect();
                    out.push_str("<code>");
                    out.push_str(&escape_html(&code_str));
                    out.push_str("</code>");
                    i = close + n;
                } else {
                    for _ in 0..n {
                        out.push('`');
                    }
                }
            }
            '*' | '_' | '~' => {
                let c0 = c;
                let mut n = 0;
                while i + n < end && chars[i + n] == c0 {
                    n += 1;
                }
                let max_len = if c0 == '~' { 2 } else { 3 };
                let before_alnum = i > start && chars[i - 1].is_alphanumeric();
                let after_alnum = i + n < end && chars[i + n].is_alphanumeric();
                let after_space = i + n >= end || chars[i + n].is_whitespace();

                if (c0 == '_' && before_alnum && after_alnum) || after_space || n == 0 {
                    for _ in 0..n {
                        out.push(c0);
                    }
                    i += n.max(1);
                    continue;
                }

                let eff = n.min(max_len);
                for _ in 0..(n - eff) {
                    out.push(c0);
                }
                let content_start = i + n;
                let mut matched = false;
                let try_lens: &[usize] = if c0 == '~' { &[2] } else { &[3, 2, 1] };
                for &len in try_lens {
                    if len > eff {
                        continue;
                    }
                    if let Some(close) = find_emphasis_close(chars, content_start, end, c0, len) {
                        let inner = render_span(chars, content_start, close);
                        match (c0, len) {
                            ('~', 2) => {
                                out.push_str("<del>");
                                out.push_str(&inner);
                                out.push_str("</del>");
                            }
                            (_, 3) => {
                                out.push_str("<strong><em>");
                                out.push_str(&inner);
                                out.push_str("</em></strong>");
                            }
                            (_, 2) => {
                                out.push_str("<strong>");
                                out.push_str(&inner);
                                out.push_str("</strong>");
                            }
                            (_, 1) => {
                                out.push_str("<em>");
                                out.push_str(&inner);
                                out.push_str("</em>");
                            }
                            _ => unreachable!(),
                        }
                        i = close + len;
                        matched = true;
                        break;
                    }
                }
                if !matched {
                    for _ in 0..eff {
                        out.push(c0);
                    }
                    i = content_start;
                }
            }
            '!' if i + 1 < end && chars[i + 1] == '[' => {
                if let Some((html, next)) = try_link(chars, i + 1, end, true) {
                    out.push_str(&html);
                    i = next;
                } else {
                    push_escaped(&mut out, c);
                    i += 1;
                }
            }
            '[' => {
                if let Some((html, next)) = try_link(chars, i, end, false) {
                    out.push_str(&html);
                    i = next;
                } else {
                    push_escaped(&mut out, c);
                    i += 1;
                }
            }
            '<' => {
                if let Some((html, next)) = try_angle(chars, i, end) {
                    out.push_str(&html);
                    i = next;
                } else {
                    out.push_str("&lt;");
                    i += 1;
                }
            }
            '&' => {
                if let Some(next) = entity_end(chars, i, end) {
                    let s: String = chars[i..next].iter().collect();
                    out.push_str(&s);
                    i = next;
                } else {
                    out.push_str("&amp;");
                    i += 1;
                }
            }
            '>' => {
                out.push_str("&gt;");
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out
}

fn find_backtick_close(chars: &[char], from: usize, end: usize, n: usize) -> Option<usize> {
    let mut i = from;
    while i < end {
        if chars[i] == '`' {
            let run_start = i;
            let mut run = 0;
            while i < end && chars[i] == '`' {
                run += 1;
                i += 1;
            }
            if run == n {
                return Some(run_start);
            }
        } else {
            i += 1;
        }
    }
    None
}

fn find_emphasis_close(chars: &[char], from: usize, end: usize, c: char, len: usize) -> Option<usize> {
    let mut i = from;
    while i + len <= end {
        if chars[i] == c {
            let mut run = 0;
            while i + run < end && chars[i + run] == c {
                run += 1;
            }
            if run >= len {
                let preceded_by_space = i == from || chars[i - 1].is_whitespace();
                let intraword_underscore = c == '_'
                    && i > from
                    && chars[i - 1].is_alphanumeric()
                    && i + len < end
                    && chars[i + len].is_alphanumeric();
                if !preceded_by_space && !intraword_underscore {
                    return Some(i);
                }
            }
            i += run.max(1);
        } else {
            i += 1;
        }
    }
    None
}

/// Find a matching `]` for `[` at `start`, respecting nesting.
fn find_matching_bracket(chars: &[char], start: usize, end: usize) -> Option<usize> {
    let mut depth = 0;
    let mut i = start;
    while i < end {
        match chars[i] {
            '[' => depth += 1,
            ']' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            '\\' => i += 1,
            _ => {}
        }
        i += 1;
    }
    None
}

fn find_matching_paren(chars: &[char], start: usize, end: usize) -> Option<usize> {
    let mut depth = 1;
    let mut i = start;
    while i < end {
        match chars[i] {
            '(' => depth += 1,
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            '\\' => i += 1,
            _ => {}
        }
        i += 1;
    }
    None
}

/// Parse `[text](dest "title")` or `![alt](dest "title")`. `start` points at the `[`.
fn try_link(chars: &[char], start: usize, end: usize, is_image: bool) -> Option<(String, usize)> {
    let close_bracket = find_matching_bracket(chars, start, end)?;
    if close_bracket + 1 >= end || chars[close_bracket + 1] != '(' {
        return None;
    }
    let close_paren = find_matching_paren(chars, close_bracket + 2, end)?;
    let label = &chars[start + 1..close_bracket];
    let dest_title: String = chars[close_bracket + 2..close_paren].iter().collect();
    let dest_title = dest_title.trim();
    let (url, title) = split_dest_title(dest_title);

    let html = if is_image {
        let alt = render_span(label, 0, label.len());
        let alt_plain = strip_tags(&alt);
        match title {
            Some(t) => format!("<img src=\"{}\" alt=\"{}\" title=\"{}\">", escape_attr(&url), escape_attr(&alt_plain), escape_attr(&t)),
            None => format!("<img src=\"{}\" alt=\"{}\">", escape_attr(&url), escape_attr(&alt_plain)),
        }
    } else {
        let text = render_span(label, 0, label.len());
        match title {
            Some(t) => format!("<a href=\"{}\" title=\"{}\">{}</a>", escape_attr(&url), escape_attr(&t), text),
            None => format!("<a href=\"{}\">{}</a>", escape_attr(&url), text),
        }
    };
    Some((html, close_paren + 1))
}

fn strip_tags(html: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(c),
            _ => {}
        }
    }
    out
}

fn split_dest_title(s: &str) -> (String, Option<String>) {
    let s = s.trim();
    let s = s.strip_prefix('<').and_then(|s| s.strip_suffix('>')).unwrap_or(s);
    if let Some(q) = s.find(['"', '\'']) {
        let (url, rest) = s.split_at(q);
        let quote = rest.chars().next().unwrap();
        let rest = &rest[1..];
        if let Some(end) = rest.rfind(quote) {
            return (url.trim().to_string(), Some(rest[..end].to_string()));
        }
    }
    (s.to_string(), None)
}

/// Handle `<https://...>`, `<user@host>`, or pass through simple inline HTML tags.
fn try_angle(chars: &[char], start: usize, end: usize) -> Option<(String, usize)> {
    let close = (start + 1..end).find(|&j| chars[j] == '>')?;
    let inner: String = chars[start + 1..close].iter().collect();
    if inner.is_empty() || inner.contains(char::is_whitespace) {
        if is_inline_html_tag(&inner) {
            return Some((format!("<{}>", inner), close + 1));
        }
        return None;
    }
    if let Some(scheme_end) = inner.find("://") {
        let scheme = &inner[..scheme_end];
        if !scheme.is_empty() && scheme.chars().all(|c| c.is_ascii_alphanumeric() || c == '+' || c == '.' || c == '-') {
            return Some((format!("<a href=\"{}\">{}</a>", escape_attr(&inner), escape_html(&inner)), close + 1));
        }
    }
    if inner.contains('@') && !inner.contains("//") {
        return Some((format!("<a href=\"mailto:{}\">{}</a>", escape_attr(&inner), escape_html(&inner)), close + 1));
    }
    if is_inline_html_tag(&inner) {
        return Some((format!("<{}>", inner), close + 1));
    }
    None
}

fn is_inline_html_tag(inner: &str) -> bool {
    let rest = inner.strip_prefix('/').unwrap_or(inner);
    let rest = rest.trim_end_matches('/');
    let mut chars = rest.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    rest.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == ' ' || c == '=' || c == '"' || c == '\'' || c == '_' || c == ':' || c == '.' || c == '/' || c == '#' || c == '%' || c == ';')
}

fn entity_end(chars: &[char], start: usize, end: usize) -> Option<usize> {
    // chars[start] == '&'
    let mut i = start + 1;
    if i < end && chars[i] == '#' {
        i += 1;
        let digit_start = i;
        while i < end && chars[i].is_ascii_hexdigit() {
            i += 1;
        }
        if i > digit_start && i < end && chars[i] == ';' {
            return Some(i + 1);
        }
        return None;
    }
    let name_start = i;
    while i < end && chars[i].is_ascii_alphanumeric() {
        i += 1;
    }
    if i > name_start && i < end && chars[i] == ';' {
        return Some(i + 1);
    }
    None
}
