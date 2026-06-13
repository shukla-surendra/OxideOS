//! docgen — converts the OxideOS `.md` documentation into a static HTML site
//! and (optionally) serves it alongside the rustdoc output for the kernel
//! and userspace crates.
//!
//! Usage:
//!   docgen build          Convert docs/**.md, README.md, CONTRIBUTING.md to HTML
//!   docgen index          (Re)generate the top-level and code-docs index pages
//!   docgen serve [port]   Serve docs_html/ over HTTP (default port 8000)

mod markdown;
mod server;

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let args: Vec<String> = env::args().collect();
    let cmd = args.get(1).map(|s| s.as_str()).unwrap_or("build");
    let root = repo_root();
    let out_dir = root.join("docs_html");

    match cmd {
        "build" => {
            build_manual(&root, &out_dir);
            build_index(&out_dir);
        }
        "index" => build_index(&out_dir),
        "serve" => {
            let port: u16 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(8000);
            server::serve(&out_dir, port);
        }
        other => {
            eprintln!("unknown command `{other}`");
            eprintln!("usage: docgen <build|index|serve> [port]");
            std::process::exit(1);
        }
    }
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("tools/docgen should live two levels under the repo root")
        .to_path_buf()
}

// ───────────────────────── manual (markdown) docs ─────────────────────────

#[derive(Default)]
struct NavNode {
    files: Vec<(String, String)>, // (title, href relative to manual/)
    dirs: BTreeMap<String, NavNode>,
}

impl NavNode {
    fn insert(&mut self, rel: &Path, title: &str) {
        let comps: Vec<String> = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect();
        let mut node = self;
        for d in &comps[..comps.len().saturating_sub(1)] {
            node = node.dirs.entry(d.clone()).or_default();
        }
        let href = comps.join("/");
        node.files.push((title.to_string(), href));
    }

    fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("<ul class=\"nav-tree\">\n");
        for (title, href) in &self.files {
            out.push_str(&format!(
                "<li><a href=\"{}\">{}</a></li>\n",
                href,
                markdown::escape_text(title)
            ));
        }
        for (name, node) in &self.dirs {
            out.push_str(&format!(
                "<li class=\"nav-dir\"><span>{}</span>\n{}</li>\n",
                markdown::escape_text(name),
                node.render()
            ));
        }
        out.push_str("</ul>\n");
        out
    }
}

fn collect_md_files(base: &Path, dir: &Path, out: &mut Vec<(PathBuf, PathBuf)>) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let path = entry.path();
        if path.is_dir() {
            collect_md_files(base, &path, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let rel = path.strip_prefix(base).unwrap().with_extension("html");
            out.push((path, rel));
        }
    }
}

fn build_manual(root: &Path, out_dir: &Path) {
    let manual_dir = out_dir.join("manual");
    let _ = fs::remove_dir_all(&manual_dir);
    fs::create_dir_all(&manual_dir).expect("failed to create docs_html/manual");
    write_style(out_dir);

    let mut files: Vec<(PathBuf, PathBuf)> = Vec::new();
    for name in ["README.md", "CONTRIBUTING.md"] {
        let p = root.join(name);
        if p.exists() {
            files.push((p, PathBuf::from(name).with_extension("html")));
        }
    }
    let docs_dir = root.join("docs");
    collect_md_files(&docs_dir, &docs_dir, &mut files);

    let mut nav = NavNode::default();
    for (src, rel) in &files {
        let content = fs::read_to_string(src).unwrap_or_default();
        let title = markdown::first_heading(&content)
            .unwrap_or_else(|| rel.file_stem().unwrap().to_string_lossy().into_owned());
        let body = markdown::to_html(&content);
        let depth_from_root = 1 + rel.parent().map(|p| p.components().count()).unwrap_or(0);
        let page = page_template(&title, &body, depth_from_root);

        let out_path = manual_dir.join(rel);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&out_path, page).unwrap();
        copy_local_assets(&content, src.parent().unwrap(), out_path.parent().unwrap());
        nav.insert(rel, &title);
    }

    let body = format!("<h1>Manual</h1>\n{}", nav.render());
    fs::write(manual_dir.join("index.html"), page_template("Manual", &body, 1)).unwrap();

    println!("Converted {} markdown file(s) into {}", files.len(), manual_dir.display());
}

/// Copy locally-referenced images (`![alt](path)` / `<img src="path">`) next to the
/// generated HTML page so relative paths keep working.
fn copy_local_assets(content: &str, src_dir: &Path, out_dir: &Path) {
    let mut paths = Vec::new();
    for (i, _) in content.char_indices() {
        if content[i..].starts_with("](") {
            if let Some(end) = content[i + 2..].find(')') {
                let inner = content[i + 2..i + 2 + end].split_whitespace().next().unwrap_or("");
                paths.push(inner.trim_matches(|c| c == '<' || c == '>').to_string());
            }
        }
    }
    for part in content.split("src=\"").skip(1) {
        if let Some(end) = part.find('"') {
            paths.push(part[..end].to_string());
        }
    }
    const IMAGE_EXTS: &[&str] = &["png", "jpg", "jpeg", "gif", "svg", "webp", "ico", "bmp"];
    for path in paths {
        if path.is_empty()
            || path.starts_with('#')
            || path.starts_with("data:")
            || path.starts_with("mailto:")
            || path.contains("://")
            || path.starts_with("//")
        {
            continue;
        }
        let is_image = Path::new(&path)
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| IMAGE_EXTS.contains(&e.to_ascii_lowercase().as_str()))
            .unwrap_or(false);
        if !is_image {
            continue;
        }
        let src_path = src_dir.join(&path);
        if src_path.is_file() {
            let dest = out_dir.join(&path);
            if let Some(parent) = dest.parent() {
                let _ = fs::create_dir_all(parent);
            }
            let _ = fs::copy(&src_path, &dest);
        }
    }
}

// ───────────────────────── top-level / code-docs index ─────────────────────────

const RUSTDOC_ASSET_DIRS: &[&str] = &["static.files", "src", "trait.impl", "type.impl"];

fn list_doc_crates(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(dir) else { return out };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if RUSTDOC_ASSET_DIRS.contains(&name.as_str()) {
            continue;
        }
        if path.join("index.html").exists() {
            out.push(name);
        }
    }
    out.sort();
    out
}

fn build_index(out_dir: &Path) {
    fs::create_dir_all(out_dir).expect("failed to create docs_html");
    write_style(out_dir);

    let code_dir = out_dir.join("code");
    let mut code_links: Vec<(String, String)> = Vec::new();
    for (label, sub) in [("Kernel", "kernel"), ("Userspace", "userspace")] {
        let crate_dir = code_dir.join(sub);
        for c in list_doc_crates(&crate_dir) {
            code_links.push((format!("{label} — {c}"), format!("{sub}/{c}/index.html")));
        }
    }

    if code_dir.exists() {
        let mut body = String::from("<h1>Code Documentation</h1>\n");
        if code_links.is_empty() {
            body.push_str("<p>No rustdoc output found. Run <code>make docs-code</code> to generate it.</p>\n");
        } else {
            body.push_str("<ul>\n");
            for (label, href) in &code_links {
                body.push_str(&format!(
                    "<li><a href=\"{}\">{}</a></li>\n",
                    href,
                    markdown::escape_text(label)
                ));
            }
            body.push_str("</ul>\n");
        }
        fs::write(code_dir.join("index.html"), page_template("Code Documentation", &body, 1)).unwrap();
    }

    let mut body = String::from("<h1>OxideOS Documentation</h1>\n<ul class=\"landing\">\n");
    body.push_str("<li><a href=\"manual/index.html\">Manual — guides, design notes and references</a></li>\n");
    if code_dir.exists() {
        body.push_str("<li><a href=\"code/index.html\">Code Documentation — rustdoc for the kernel and userspace crates</a></li>\n");
    } else {
        body.push_str("<li>Code Documentation — run <code>make docs-code</code> to generate rustdoc output</li>\n");
    }
    body.push_str("</ul>\n");
    fs::write(out_dir.join("index.html"), page_template("OxideOS Documentation", &body, 0)).unwrap();
}

// ───────────────────────── HTML page template / styling ─────────────────────────

/// `depth_from_root` is the number of directories the page lives under `docs_html/`
/// (e.g. `docs_html/index.html` -> 0, `docs_html/manual/index.html` -> 1,
/// `docs_html/manual/study/x.html` -> 2).
fn page_template(title: &str, body: &str, depth_from_root: usize) -> String {
    let root = "../".repeat(depth_from_root);
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>{title} — OxideOS Docs</title>
<link rel="stylesheet" href="{root}style.css">
</head>
<body>
<header class="topbar">
<a class="brand" href="{root}index.html">OxideOS Docs</a>
<nav>
<a href="{root}manual/index.html">Manual</a>
<a href="{root}code/index.html">Code Docs</a>
</nav>
</header>
<main>
{body}
</main>
</body>
</html>
"#,
        title = markdown::escape_text(title),
        root = root,
        body = body,
    )
}

fn write_style(out_dir: &Path) {
    let css = r#":root {
  color-scheme: light dark;
  --fg: #1b1f24;
  --bg: #ffffff;
  --muted: #6a737d;
  --border: #d0d7de;
  --accent: #b85c00;
  --code-bg: #f6f8fa;
}
@media (prefers-color-scheme: dark) {
  :root {
    --fg: #e6edf3;
    --bg: #0d1117;
    --muted: #9198a1;
    --border: #30363d;
    --accent: #ffa657;
    --code-bg: #161b22;
  }
}
* { box-sizing: border-box; }
body {
  margin: 0;
  font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Helvetica, Arial, sans-serif;
  color: var(--fg);
  background: var(--bg);
  line-height: 1.6;
}
.topbar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0.75rem 1.5rem;
  border-bottom: 1px solid var(--border);
}
.topbar .brand { font-weight: 700; color: var(--fg); text-decoration: none; }
.topbar nav a { margin-left: 1.25rem; color: var(--accent); text-decoration: none; }
.topbar nav a:hover { text-decoration: underline; }
main {
  max-width: 60rem;
  margin: 0 auto;
  padding: 1.5rem 2rem 4rem;
}
h1, h2, h3, h4, h5, h6 { line-height: 1.25; }
h1 { border-bottom: 1px solid var(--border); padding-bottom: 0.3rem; }
h2 { border-bottom: 1px solid var(--border); padding-bottom: 0.2rem; }
a { color: var(--accent); }
code {
  background: var(--code-bg);
  padding: 0.15em 0.35em;
  border-radius: 4px;
  font-size: 0.9em;
}
pre {
  background: var(--code-bg);
  padding: 0.9rem 1rem;
  border-radius: 6px;
  overflow-x: auto;
  border: 1px solid var(--border);
}
pre code { background: none; padding: 0; }
table { border-collapse: collapse; width: 100%; margin: 1rem 0; }
th, td { border: 1px solid var(--border); padding: 0.4rem 0.7rem; text-align: left; }
th { background: var(--code-bg); }
blockquote {
  margin: 1rem 0;
  padding: 0.2rem 1rem;
  border-left: 4px solid var(--border);
  color: var(--muted);
}
img { max-width: 100%; }
hr { border: none; border-top: 1px solid var(--border); margin: 1.5rem 0; }
ul.landing { list-style: none; padding: 0; }
ul.landing li { margin: 0.75rem 0; }
ul.nav-tree { list-style: none; padding-left: 1rem; }
ul.nav-tree > li { margin: 0.2rem 0; }
li.nav-dir > span { font-weight: 600; color: var(--muted); }
"#;
    fs::write(out_dir.join("style.css"), css).expect("failed to write style.css");
}
