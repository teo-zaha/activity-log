//! A description's layout: its lines as text, headings and list items of plain, bold, italic
//! and linked runs (`Line`), and from them the blocks the dashboard renders. Lines come from
//! HTML (`html`) or from plain text with the marks of Markdown and of the text Outlook writes
//! for a formatted mail: `*`, `-`, `•` and numbered items, `# headings`, `**bold**`,
//! `*italic*`, `[label](url)` and `label<url>`.

mod html;

#[cfg(test)]
mod tests;

use reqwest::Url;

use crate::model::{DescriptionBlock, ListItem, TextSegment};

use super::url_spans;

pub(super) use html::html_lines;
pub(crate) use html::looks_like_html;

/// A list nested deeper than this is shown at this depth.
const MAX_DEPTH: usize = 4;
/// A label longer than this before `<url>` is a sentence, not the link's own text.
const MAX_LABEL_WORDS: usize = 8;
/// Text in brackets alone on a line longer than this is no image's alt text.
const MAX_ALT_CHARS: usize = 80;
/// What a link in brackets alone on a line ends in when it is an image's source.
const IMAGE_EXTENSIONS: [&str; 8] = ["jpg", "jpeg", "png", "gif", "webp", "avif", "bmp", "svg"];
/// What `plain_text` puts between blocks; the cap counts it.
const BLOCK_SEPARATOR: &str = "\n\n";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct Run {
    pub text: String,
    pub href: Option<String>,
    pub strong: bool,
    pub emphasis: bool,
}

impl Run {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn link(text: impl Into<String>, href: String) -> Self {
        Self {
            text: text.into(),
            href: Some(href),
            ..Self::default()
        }
    }

    fn same_style(&self, other: &Self) -> bool {
        self.href == other.href && self.strong == other.strong && self.emphasis == other.emphasis
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LineKind {
    Text,
    Heading,
    /// `indent` orders the items of a list by depth: spaces in text, nesting in HTML.
    Item {
        indent: usize,
        ordered: bool,
    },
    /// An image on a line of its own, which has no runs.
    Image {
        src: Option<String>,
        alt: Option<String>,
    },
    Blank,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Line {
    pub kind: LineKind,
    pub runs: Vec<Run>,
}

impl Line {
    pub fn blank() -> Self {
        Self {
            kind: LineKind::Blank,
            runs: Vec::new(),
        }
    }

    pub fn image(src: Option<String>, alt: Option<String>) -> Self {
        Self {
            kind: LineKind::Image { src, alt },
            runs: Vec::new(),
        }
    }

    pub fn text(&self) -> String {
        self.runs.iter().map(|run| run.text.as_str()).collect()
    }

    fn is_empty(&self) -> bool {
        self.runs.iter().all(|run| run.text.trim().is_empty())
    }
}

/// The lines of a plain-text description.
pub(super) fn plain_lines(text: &str) -> Vec<Line> {
    text.lines().map(plain_line).collect()
}

fn plain_line(line: &str) -> Line {
    let line = line.trim_end();
    let body = line.trim_start();
    if body.is_empty() {
        return Line::blank();
    }
    if let Some(line) = bracketed(body) {
        return line;
    }
    let indent = line[..line.len() - body.len()]
        .chars()
        .map(|c| if c == '\t' { 4 } else { 1 })
        .sum();
    if let Some(rest) = heading(body) {
        return Line {
            kind: LineKind::Heading,
            runs: inline_runs(rest),
        };
    }
    match item(body, indent) {
        Some((ordered, rest)) => Line {
            kind: LineKind::Item { indent, ordered },
            runs: inline_runs(rest),
        },
        None => Line {
            kind: LineKind::Text,
            runs: inline_runs(body),
        },
    }
}

/// Outlook writes an image in plain text as a line of its own in brackets: its source link,
/// `[https://…/logo.jpg]`, or for an attachment the feed lacks its alt text, `[Company Logo]`,
/// or its `cid:` name. Another link alone in brackets is a link; text that looks like no alt
/// text (a long sentence, a `mailto:`, `[x]`) stays text. Read from the line as written, so
/// that a Markdown link or emphasis in brackets stays what it is.
fn bracketed(body: &str) -> Option<Line> {
    let inner = body.strip_prefix('[')?.strip_suffix(']')?.trim();
    let lower = inner.to_ascii_lowercase();
    let token = !inner.contains(char::is_whitespace);
    if lower.starts_with("https://") || lower.starts_with("http://") {
        if !token {
            return None;
        }
        if let Some(src) = image_src(inner).filter(|src| is_image_file(src)) {
            return Some(Line::image(Some(src), None));
        }
        let href = web_link(inner)?;
        return Some(Line {
            kind: LineKind::Text,
            runs: vec![Run::link(inner, href)],
        });
    }
    if token && lower.starts_with("cid:") {
        return Some(Line::image(None, None));
    }
    // `mailto:jane@example.org` and the like name no image; `Logo:` may.
    let scheme_like = token
        && inner
            .split_once(':')
            .is_some_and(|(_, rest)| !rest.is_empty());
    let is_alt = (2..=MAX_ALT_CHARS).contains(&inner.chars().count())
        && inner.contains(char::is_alphabetic)
        && !inner.contains(['[', ']'])
        && !lower.contains("://")
        && !scheme_like;
    is_alt.then(|| Line::image(None, Some(inner.to_string())))
}

/// `raw` as an image's source: an absolute `https://` link with a host, as written.
pub(super) fn image_src(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if !raw
        .get(..8)
        .is_some_and(|scheme| scheme.eq_ignore_ascii_case("https://"))
    {
        return None;
    }
    Url::parse(raw)
        .ok()
        .filter(|url| url.scheme() == "https")
        .filter(|url| url.host_str().is_some_and(|host| !host.is_empty()))
        .map(|url| url.to_string())
}

/// Whether the link's path ends in an image's file extension.
fn is_image_file(src: &str) -> bool {
    Url::parse(src).is_ok_and(|url| {
        url.path().rsplit_once('.').is_some_and(|(_, extension)| {
            IMAGE_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str())
        })
    })
}

/// `# Heading` to `###### Heading`.
fn heading(body: &str) -> Option<&str> {
    let level = body.chars().take_while(|&c| c == '#').count();
    let rest = body[level..].strip_prefix(' ')?.trim();
    ((1..=6).contains(&level) && !rest.is_empty()).then_some(rest)
}

/// A list item's marker: whether the list is numbered, and the item's text. Outlook marks the
/// items of nested lists `o` and `§`, which count only when indented.
fn item(body: &str, indent: usize) -> Option<(bool, &str)> {
    let first = body.chars().next()?;
    let (ordered, marker_len) = match first {
        '*' | '-' | '•' | '·' | '▪' | '◦' | '–' => (false, first.len_utf8()),
        'o' | '§' if indent > 0 => (false, first.len_utf8()),
        '0'..='9' => {
            let digits = body.chars().take_while(char::is_ascii_digit).count();
            if digits > 3 || !matches!(body[digits..].chars().next(), Some('.' | ')')) {
                return None;
            }
            (true, digits + 1)
        }
        _ => return None,
    };
    let rest = &body[marker_len..];
    if !rest.starts_with(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim();
    (!rest.is_empty()).then_some((ordered, rest))
}

/// The runs of a line of text: its links (bare, `[label](url)` and `label<url>`) and the
/// `**bold**` and `*italic*` text between them.
pub(super) fn inline_runs(text: &str) -> Vec<Run> {
    let mut runs = Vec::new();
    let mut copied = 0;
    for span in url_spans(text) {
        if span.start < copied {
            continue;
        }
        let written = &text[span.clone()];
        let Some(href) = web_link(written) else {
            continue;
        };
        let before = &text[copied..span.start];
        let after = &text[span.end..];
        if let (Some(before), Some(_)) = (before.strip_suffix("]("), after.strip_prefix(')'))
            && let Some(open) = before.rfind('[')
        {
            styled_runs(&mut runs, &before[..open]);
            let label = before[open + 1..].trim();
            runs.push(Run::link(
                if label.is_empty() { written } else { label },
                href,
            ));
            copied = span.end + 1;
        } else if let (Some(before), Some(_)) = (before.strip_suffix('<'), after.strip_prefix('>'))
        {
            copied = span.end + 1;
            // `https://x<https://x>`: a link used as its own label, already a link.
            if before.is_empty()
                && runs
                    .last()
                    .is_some_and(|run| run.href.as_ref() == Some(&href))
            {
                continue;
            }
            let (keep, label) = label_before(before);
            styled_runs(&mut runs, keep);
            runs.push(match label {
                Some(label) => Run::link(label, href),
                None if before.trim().is_empty() => Run::link(written, href),
                None => Run::link(link_host(&href), href),
            });
        } else {
            styled_runs(&mut runs, before);
            runs.push(Run::link(written, href));
            copied = span.end;
        }
    }
    styled_runs(&mut runs, &text[copied..]);
    runs
}

/// `written` as an http(s) link with a host.
pub(super) fn web_link(written: &str) -> Option<String> {
    Url::parse(written)
        .ok()
        .filter(|url| matches!(url.scheme(), "http" | "https"))
        .filter(|url| url.host_str().is_some_and(|host| !host.is_empty()))
        .map(|url| url.to_string())
}

fn link_host(href: &str) -> String {
    Url::parse(href)
        .ok()
        .and_then(|url| {
            url.host_str()
                .map(|host| host.trim_start_matches("www.").to_string())
        })
        .unwrap_or_else(|| href.to_string())
}

/// Outlook writes a link as its label followed by `<url>`: the text to keep before the label,
/// and the label. The label starts after the last sentence mark; a longer one is a sentence,
/// which stays text, and the link is named by its host.
fn label_before(before: &str) -> (&str, Option<&str>) {
    let start = [": ", ". ", "! ", "? ", " - ", " – ", " | ", "(", "\t", "> "]
        .iter()
        .filter_map(|mark| before.rfind(mark).map(|at| at + mark.len()))
        .max()
        .unwrap_or(0);
    let label = before[start..].trim();
    if label.is_empty() || label.split_whitespace().count() > MAX_LABEL_WORDS {
        return (before, None);
    }
    let label_start = start + before[start..].find(label).unwrap_or(0);
    (&before[..label_start], Some(label))
}

/// Appends `text` with its `**bold**` and `*italic*` parts, without Outlook's `<mailto:…>`.
fn styled_runs(runs: &mut Vec<Run>, text: &str) {
    let text = without_mailto(text);
    let mut plain = String::new();
    let mut rest = text.as_str();
    while let Some(c) = rest.chars().next() {
        let marker = if rest.starts_with("**") { "**" } else { "*" };
        if c == '*'
            && let Some(inner) = emphasized(&rest[marker.len()..], marker)
        {
            push_run(runs, Run::plain(std::mem::take(&mut plain)));
            push_run(
                runs,
                Run {
                    text: inner.to_string(),
                    strong: marker == "**",
                    emphasis: marker == "*",
                    ..Run::default()
                },
            );
            rest = &rest[marker.len() * 2 + inner.len()..];
            continue;
        }
        plain.push(c);
        rest = &rest[c.len_utf8()..];
    }
    push_run(runs, Run::plain(plain));
}

/// The text up to the closing `marker`, when it neither starts nor ends with a space.
fn emphasized<'a>(rest: &'a str, marker: &str) -> Option<&'a str> {
    let inner = &rest[..rest.find(marker)?];
    let tight = !inner.is_empty()
        && !inner.starts_with(char::is_whitespace)
        && !inner.ends_with(char::is_whitespace)
        && !inner.contains('*');
    tight.then_some(inner)
}

fn without_mailto(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("<mailto:") {
        let Some(end) = rest[start..].find('>') else {
            break;
        };
        out.push_str(&rest[..start]);
        rest = &rest[start + end + 1..];
    }
    out.push_str(rest);
    out
}

/// Adds `run` to the line, joined to the last run when they look the same.
pub(super) fn push_run(runs: &mut Vec<Run>, run: Run) {
    if run.text.is_empty() {
        return;
    }
    match runs.last_mut() {
        Some(last) if last.same_style(&run) => last.text.push_str(&run.text),
        _ => runs.push(run),
    }
}

/// The blocks of `lines`, at most `max_chars` characters of text, the cut marked with `…`.
pub(super) fn blocks(lines: &[Line], max_chars: usize) -> Vec<DescriptionBlock> {
    let mut builder = BlockBuilder::default();
    for line in lines {
        builder.line(line);
    }
    cap(builder.finish(), max_chars)
}

#[derive(Default)]
struct BlockBuilder {
    blocks: Vec<DescriptionBlock>,
    paragraph: Vec<Run>,
    list: Option<OpenList>,
}

struct OpenList {
    ordered: bool,
    /// Each item's runs with its indent.
    items: Vec<(usize, Vec<Run>)>,
}

impl BlockBuilder {
    fn line(&mut self, line: &Line) {
        if let LineKind::Image { src, alt } = &line.kind {
            self.flush_paragraph();
            self.flush_list();
            self.blocks.push(DescriptionBlock::Image {
                src: src.clone(),
                alt: alt.clone(),
            });
            return;
        }
        if line.is_empty() {
            self.flush_paragraph();
            self.flush_list();
            return;
        }
        let runs = trimmed(&line.runs);
        match line.kind {
            LineKind::Blank | LineKind::Image { .. } => {}
            LineKind::Heading => {
                self.flush_paragraph();
                self.flush_list();
                self.blocks.push(DescriptionBlock::Heading {
                    segments: segments(runs),
                });
            }
            LineKind::Item { indent, ordered } => {
                self.flush_paragraph();
                let continues = self.list.as_ref().is_some_and(|list| {
                    list.ordered == ordered
                        || list.items.first().is_some_and(|(first, _)| indent > *first)
                });
                if !continues {
                    self.flush_list();
                }
                self.list
                    .get_or_insert_with(|| OpenList {
                        ordered,
                        items: Vec::new(),
                    })
                    .items
                    .push((indent, runs));
            }
            LineKind::Text => {
                self.flush_list();
                if !self.paragraph.is_empty() {
                    push_run(&mut self.paragraph, Run::plain("\n"));
                }
                for run in runs {
                    push_run(&mut self.paragraph, run);
                }
            }
        }
    }

    fn flush_paragraph(&mut self) {
        if !self.paragraph.is_empty() {
            let runs = std::mem::take(&mut self.paragraph);
            self.blocks.push(DescriptionBlock::Paragraph {
                segments: segments(runs),
            });
        }
    }

    fn flush_list(&mut self) {
        let Some(OpenList { ordered, items }) = self.list.take() else {
            return;
        };
        let mut indents = items.iter().map(|(indent, _)| *indent).collect::<Vec<_>>();
        indents.sort_unstable();
        indents.dedup();
        let items = items
            .into_iter()
            .map(|(indent, runs)| ListItem {
                depth: indents
                    .iter()
                    .position(|&other| other == indent)
                    .unwrap_or(0)
                    .min(MAX_DEPTH) as u8,
                segments: segments(runs),
            })
            .collect();
        self.blocks.push(DescriptionBlock::List { ordered, items });
    }

    fn finish(mut self) -> Vec<DescriptionBlock> {
        self.flush_paragraph();
        self.flush_list();
        self.blocks
    }
}

/// `runs` without the white space at their start and end.
fn trimmed(runs: &[Run]) -> Vec<Run> {
    let mut runs = runs.to_vec();
    if let Some(first) = runs.first_mut() {
        first.text = first.text.trim_start().to_string();
    }
    if let Some(last) = runs.last_mut() {
        last.text = last.text.trim_end().to_string();
    }
    runs.retain(|run| !run.text.is_empty());
    runs
}

/// The runs as segments; a link in several styles is one link.
fn segments(runs: Vec<Run>) -> Vec<TextSegment> {
    let mut segments: Vec<TextSegment> = Vec::new();
    for run in runs {
        match (run.href, segments.last_mut()) {
            (Some(href), Some(TextSegment::Link { text, href: last })) if *last == href => {
                text.push_str(&run.text);
            }
            (Some(href), _) => segments.push(TextSegment::link(run.text, href)),
            (None, _) => segments.push(TextSegment::Text {
                text: run.text,
                strong: run.strong,
                emphasis: run.emphasis,
            }),
        }
    }
    segments
}

/// Keeps the first `max_chars` characters of text, counting the separator between blocks. A
/// link the cut would split goes as a whole: half a link would still be one, to elsewhere. An
/// image costs its alt text, at least one character, and goes as a whole too.
fn cap(blocks: Vec<DescriptionBlock>, max_chars: usize) -> Vec<DescriptionBlock> {
    let separator = BLOCK_SEPARATOR.chars().count();
    let mut left = max_chars;
    let mut out = Vec::new();
    for block in blocks {
        if !out.is_empty() {
            if left <= separator {
                break;
            }
            left -= separator;
        }
        let (block, cut) = match block {
            DescriptionBlock::Paragraph { segments } => {
                let (segments, cut) = cap_segments(segments, &mut left);
                (DescriptionBlock::Paragraph { segments }, cut)
            }
            DescriptionBlock::Heading { segments } => {
                let (segments, cut) = cap_segments(segments, &mut left);
                (DescriptionBlock::Heading { segments }, cut)
            }
            DescriptionBlock::List { ordered, items } => {
                let mut kept = Vec::new();
                let mut cut = false;
                for item in items {
                    let (segments, item_cut) = cap_segments(item.segments, &mut left);
                    if !segments.is_empty() {
                        kept.push(ListItem { segments, ..item });
                    }
                    if item_cut {
                        cut = true;
                        break;
                    }
                }
                (
                    DescriptionBlock::List {
                        ordered,
                        items: kept,
                    },
                    cut,
                )
            }
            DescriptionBlock::Image { src, alt } => {
                let cost = alt.as_deref().map_or(0, |alt| alt.chars().count()).max(1);
                if cost > left {
                    break;
                }
                left -= cost;
                (DescriptionBlock::Image { src, alt }, false)
            }
        };
        if !is_empty(&block) {
            out.push(block);
        }
        if cut {
            break;
        }
    }
    out
}

/// The segments that fit in `left` characters, the last ending in `…` when some did not fit,
/// and whether some did not.
fn cap_segments(segments: Vec<TextSegment>, left: &mut usize) -> (Vec<TextSegment>, bool) {
    let mut kept = Vec::new();
    for segment in segments {
        let len = match &segment {
            TextSegment::Text { text, .. } | TextSegment::Link { text, .. } => text.chars().count(),
        };
        if len <= *left {
            *left -= len;
            kept.push(segment);
            continue;
        }
        if let TextSegment::Text {
            text,
            strong,
            emphasis,
        } = segment
            && *left > 0
        {
            let cut = text
                .char_indices()
                .nth(*left - 1)
                .map_or(text.len(), |(index, _)| index);
            kept.push(TextSegment::Text {
                text: text[..cut].to_string(),
                strong,
                emphasis,
            });
        }
        match kept.last_mut() {
            Some(TextSegment::Text { text, .. }) => *text = format!("{}…", text.trim_end()),
            _ => kept.push(TextSegment::text("…")),
        }
        *left = 0;
        return (kept, true);
    }
    (kept, false)
}

fn is_empty(block: &DescriptionBlock) -> bool {
    match block {
        DescriptionBlock::Paragraph { segments } | DescriptionBlock::Heading { segments } => {
            segments.is_empty()
        }
        DescriptionBlock::List { items, .. } => items.is_empty(),
        DescriptionBlock::Image { .. } => false,
    }
}

/// `blocks` as plain text, for search and exports: items marked `•` or numbered, links that
/// have a label written `label <url>`, images as their alt text.
pub(super) fn plain_text(blocks: &[DescriptionBlock]) -> String {
    let text = |segments: &[TextSegment]| -> String {
        segments
            .iter()
            .map(|segment| match segment {
                TextSegment::Text { text, .. } => text.clone(),
                TextSegment::Link { text, href } if web_link(text).as_deref() == Some(href) => {
                    text.clone()
                }
                TextSegment::Link { text, href } => format!("{text} <{href}>"),
            })
            .collect()
    };
    blocks
        .iter()
        .map(|block| match block {
            DescriptionBlock::Paragraph { segments } | DescriptionBlock::Heading { segments } => {
                text(segments)
            }
            DescriptionBlock::List { ordered, items } => {
                let mut number = 0;
                items
                    .iter()
                    .map(|item| {
                        let marker = if *ordered && item.depth == 0 {
                            number += 1;
                            format!("{number}.")
                        } else {
                            "•".to_string()
                        };
                        let indent = "  ".repeat(usize::from(item.depth));
                        format!("{indent}{marker} {}", text(&item.segments))
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
            DescriptionBlock::Image { alt, .. } => alt.clone().unwrap_or_default(),
        })
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join(BLOCK_SEPARATOR)
}
