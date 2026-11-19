//! The lines of an HTML description (Google Calendar writes HTML into `DESCRIPTION`, Outlook
//! into `X-ALT-DESC`). Only its structure is read: paragraphs, line breaks, headings, list
//! items, bold, italic, http(s) links and images. Everything else, scripts and styles
//! included, is dropped; no markup reaches the dashboard.

use super::{Line, LineKind, Run, image_src, item, push_run, web_link};

/// Elements that make a text HTML, when written as tags: with their closing tag, for those
/// that have none alone, and for those whose closing tag may be left out (`<p>`, `<li>`) when
/// they come twice. A word in angle brackets (`<bravo>`, `a<b and c>d`) is no tag.
const HTML_ELEMENTS: [&str; 30] = [
    "a",
    "b",
    "strong",
    "i",
    "em",
    "u",
    "span",
    "font",
    "p",
    "div",
    "br",
    "hr",
    "img",
    "ul",
    "ol",
    "li",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "pre",
    "blockquote",
    "table",
    "tr",
    "td",
    "th",
    "html",
    "body",
];
const VOID_ELEMENTS: [&str; 3] = ["br", "hr", "img"];
const UNCLOSED_ELEMENTS: [&str; 2] = ["p", "li"];

/// Elements whose content is no text to show.
const HIDDEN: [&str; 5] = ["head", "script", "style", "title", "template"];

pub(crate) fn looks_like_html(text: &str) -> bool {
    let mut opened = Vec::new();
    let mut closed = std::collections::HashSet::new();
    let mut rest = text;
    while let Some(at) = rest.find('<') {
        rest = &rest[at..];
        let Some((tag, after)) = Tag::parse(rest) else {
            rest = &rest[1..];
            continue;
        };
        if HTML_ELEMENTS.contains(&tag.name.as_str()) {
            if tag.closing {
                closed.insert(tag.name);
            } else if VOID_ELEMENTS.contains(&tag.name.as_str()) {
                return true;
            } else {
                opened.push(tag.name);
            }
        }
        rest = after;
    }
    opened.iter().any(|name| {
        closed.contains(name)
            || UNCLOSED_ELEMENTS.contains(&name.as_str())
                && opened.iter().filter(|other| *other == name).count() > 1
    })
}

pub(in super::super) fn html_lines(html: &str) -> Vec<Line> {
    let mut lines = Lines::default();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(comment) = rest.strip_prefix("<!--") {
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
            continue;
        }
        if rest.starts_with('<')
            && let Some((tag, after)) = Tag::parse(rest)
        {
            rest = after;
            if !tag.closing && HIDDEN.contains(&tag.name.as_str()) {
                rest = skip_element(rest, &tag.name);
            } else {
                lines.tag(&tag);
            }
            continue;
        }
        // Text up to the next tag; a `<` that starts none is text too.
        let first = rest.chars().next().map_or(1, char::len_utf8);
        let end = rest[first..].find('<').map_or(rest.len(), |at| at + first);
        lines.text(&decode(&rest[..end]));
        rest = &rest[end..];
    }
    lines.finish()
}

/// What follows the closing tag of `name`.
fn skip_element<'a>(rest: &'a str, name: &str) -> &'a str {
    let lower = rest.to_ascii_lowercase();
    lower
        .find(&format!("</{name}"))
        .and_then(|start| rest[start..].find('>').map(|end| &rest[start + end + 1..]))
        .unwrap_or("")
}

struct Tag<'a> {
    name: String,
    closing: bool,
    attributes: &'a str,
}

impl<'a> Tag<'a> {
    /// The tag at the start of `text` (`<…>`, also `<!…>`, which has no name), and what follows.
    fn parse(text: &'a str) -> Option<(Self, &'a str)> {
        let inner = &text[1..];
        let closing = inner.starts_with('/');
        let inner = inner.trim_start_matches('/');
        let first = inner.chars().next()?;
        if !first.is_ascii_alphabetic() && first != '!' {
            return None;
        }
        let end = tag_end(inner)?;
        // `-` and `:` belong to names too: `<img-caption>`, Outlook's `<o:p>`.
        let name_len = inner
            .find(|c: char| !c.is_ascii_alphanumeric() && !matches!(c, '-' | ':'))
            .unwrap_or(end)
            .min(end);
        // A name ends where the tag goes on: `<br@example.com>` is an address, no `<br>`.
        let ends_there = inner[name_len..]
            .chars()
            .next()
            .is_none_or(|c| c.is_whitespace() || matches!(c, '/' | '>'));
        if first != '!' && !ends_there {
            return None;
        }
        let tag = Tag {
            name: inner[..name_len].to_ascii_lowercase(),
            closing,
            attributes: &inner[name_len..end],
        };
        Some((tag, &inner[end + 1..]))
    }

    /// The value of the attribute `name`.
    fn attribute(&self, name: &str) -> Option<String> {
        let mut rest = self.attributes;
        loop {
            rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
            if rest.is_empty() {
                return None;
            }
            let name_len = rest
                .find(|c: char| c.is_whitespace() || matches!(c, '=' | '>' | '/'))
                .unwrap_or(rest.len());
            let (key, after) = rest.split_at(name_len);
            let after = after.trim_start();
            let (value, next) = match after.strip_prefix('=') {
                Some(value) => {
                    let value = value.trim_start();
                    match value.chars().next() {
                        Some(quote @ ('"' | '\'')) => {
                            let end = value[1..].find(quote)? + 1;
                            (&value[1..end], &value[end + 1..])
                        }
                        _ => {
                            let end = value.find(char::is_whitespace).unwrap_or(value.len());
                            value.split_at(end)
                        }
                    }
                }
                None => ("", after),
            };
            if key.eq_ignore_ascii_case(name) {
                return Some(decode(value));
            }
            if next.len() == rest.len() {
                return None;
            }
            rest = next;
        }
    }
}

/// Where the tag that `inner` starts closes, outside quoted attribute values.
fn tag_end(inner: &str) -> Option<usize> {
    let mut quote = None;
    for (index, c) in inner.char_indices() {
        match (quote, c) {
            (None, '"' | '\'') => quote = Some(c),
            (Some(open), _) if c == open => quote = None,
            (None, '>') => return Some(index),
            _ => {}
        }
    }
    None
}

#[derive(Default)]
struct Lines {
    lines: Vec<Line>,
    current: Vec<Run>,
    kind: Option<LineKind>,
    /// Whether the text so far ends in white space.
    space: bool,
    strong: u32,
    emphasis: u32,
    href: Option<String>,
    /// Whether each open list is numbered.
    lists: Vec<bool>,
    /// Images inside a list, shown after it: a block of its own would split the list.
    list_images: Vec<Line>,
    pre: u32,
    /// Outlook writes every line as a paragraph without spacing (`MsoNormal`), and a blank
    /// line as an empty one.
    paragraph_is_line: bool,
}

impl Lines {
    fn text(&mut self, text: &str) {
        if self.pre > 0 {
            for (index, part) in text.split('\n').enumerate() {
                if index > 0 {
                    self.break_line();
                }
                self.push(part);
            }
            return;
        }
        let mut out = String::new();
        for c in text.chars() {
            if !c.is_whitespace() {
                out.push(c);
            } else if !(self.space || out.ends_with(' ') || out.is_empty() && self.at_line_start())
            {
                out.push(' ');
            }
            self.space =
                c.is_whitespace() && (self.space || !out.is_empty() || !self.at_line_start());
        }
        self.push(&out);
    }

    /// White space between cells.
    fn space(&mut self) {
        if !self.space && !self.at_line_start() {
            self.push(" ");
            self.space = true;
        }
    }

    fn at_line_start(&self) -> bool {
        self.current.iter().all(|run| run.text.is_empty())
    }

    fn push(&mut self, text: &str) {
        push_run(
            &mut self.current,
            Run {
                text: text.to_string(),
                href: self.href.clone(),
                strong: self.strong > 0,
                emphasis: self.emphasis > 0,
            },
        );
    }

    fn tag(&mut self, tag: &Tag) {
        let open = !tag.closing;
        match tag.name.as_str() {
            "br" => {
                self.break_line();
            }
            "p" if open => {
                self.break_line();
                let class = tag.attribute("class").unwrap_or_default();
                self.paragraph_is_line = class.to_ascii_lowercase().starts_with("mso");
                if let Some(level) = tag.attribute("style").as_deref().and_then(mso_list_level) {
                    self.kind = Some(LineKind::Item {
                        indent: level,
                        ordered: false,
                    });
                }
            }
            "p" => {
                let had_text = self.break_line();
                if !had_text || !self.paragraph_is_line {
                    self.blank();
                }
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.break_line();
                if open {
                    self.kind = Some(LineKind::Heading);
                } else {
                    self.blank();
                }
            }
            "ul" | "ol" => {
                self.break_line();
                if open {
                    self.lists.push(tag.name == "ol");
                } else {
                    self.lists.pop();
                    // An item that ended without text leaves its kind to no text after the list.
                    self.kind = None;
                    if self.lists.is_empty() {
                        self.lines.append(&mut self.list_images);
                        self.blank();
                    }
                }
            }
            "li" => {
                self.break_line();
                if open {
                    self.kind = Some(LineKind::Item {
                        indent: self.lists.len().saturating_sub(1),
                        ordered: self.lists.last().copied().unwrap_or(false),
                    });
                }
            }
            "pre" => {
                self.break_line();
                self.pre = if open {
                    self.pre + 1
                } else {
                    self.pre.saturating_sub(1)
                };
            }
            "hr" => {
                self.break_line();
                self.blank();
            }
            "div" | "tr" | "table" | "blockquote" | "section" | "article" | "header" | "footer"
            | "center" | "dl" | "dt" | "dd" => {
                self.break_line();
            }
            "td" | "th" => self.space(),
            "b" | "strong" => self.strong = count(self.strong, open),
            "i" | "em" => self.emphasis = count(self.emphasis, open),
            "a" if open => {
                self.href = tag.attribute("href").and_then(|href| web_link(href.trim()));
            }
            "a" => self.href = None,
            "img" if open => self.image(tag),
            _ => {}
        }
    }

    /// An image on a line of its own: its https source, else its alt text alone (an attachment,
    /// `cid:`). An empty alt text marks it decorative. A link around it is dropped, and so are
    /// images that are not shown: tracking pixels and hidden ones. An image in a list comes
    /// after the list.
    fn image(&mut self, tag: &Tag) {
        if is_invisible(tag) {
            return;
        }
        let src = tag.attribute("src").as_deref().and_then(image_src);
        let alt = tag
            .attribute("alt")
            .map(|alt| alt.split_whitespace().collect::<Vec<_>>().join(" "));
        if src.is_none() && alt.as_deref().is_none_or(str::is_empty) {
            return;
        }
        let image = Line::image(src, alt);
        if self.lists.is_empty() {
            self.break_line();
            self.lines.push(image);
        } else {
            self.list_images.push(image);
        }
    }

    /// Ends the current line; whether it had text. An empty line keeps its kind, for the text
    /// of an item that comes in a paragraph of its own.
    fn break_line(&mut self) -> bool {
        self.space = false;
        let runs = std::mem::take(&mut self.current);
        if runs.iter().all(|run| run.text.trim().is_empty()) {
            return false;
        }
        let kind = self.kind.take().unwrap_or(LineKind::Text);
        self.lines.push(outlook_item(Line { kind, runs }));
        true
    }

    fn blank(&mut self) {
        if self
            .lines
            .last()
            .is_some_and(|line| line.kind != LineKind::Blank)
        {
            self.lines.push(Line::blank());
        }
    }

    fn finish(mut self) -> Vec<Line> {
        self.break_line();
        self.lines.append(&mut self.list_images);
        while self
            .lines
            .last()
            .is_some_and(|line| line.kind == LineKind::Blank)
        {
            self.lines.pop();
        }
        self.lines
    }
}

/// A tracking pixel or a hidden image: `width` or `height` of 0 or 1 in its attributes or its
/// style, `hidden`, `display:none`, `visibility:hidden` or `opacity:0`. Hidden parents are not
/// looked at.
fn is_invisible(tag: &Tag) -> bool {
    let tiny = |size: &str| {
        size.trim()
            .trim_end_matches("!important")
            .trim()
            .trim_end_matches("px")
            .trim()
            .parse::<f32>()
            .is_ok_and(|size| size <= 1.0)
    };
    if tag.attribute("hidden").is_some()
        || tag.attribute("width").as_deref().is_some_and(tiny)
        || tag.attribute("height").as_deref().is_some_and(tiny)
    {
        return true;
    }
    let style = tag
        .attribute("style")
        .unwrap_or_default()
        .to_ascii_lowercase();
    style.split(';').any(|declaration| {
        let Some((property, value)) = declaration.split_once(':') else {
            return false;
        };
        let value = value.trim().trim_end_matches("!important").trim();
        match property.trim() {
            "width" | "height" | "max-width" | "max-height" => tiny(value),
            "display" => value == "none",
            "visibility" => value == "hidden",
            "opacity" => value.parse::<f32>().is_ok_and(|opacity| opacity <= 0.0),
            _ => false,
        }
    })
}

fn count(depth: u32, open: bool) -> u32 {
    if open {
        depth + 1
    } else {
        depth.saturating_sub(1)
    }
}

/// `mso-list:l0 level2 lfo1`: an item of an Outlook list, at depth 1.
fn mso_list_level(style: &str) -> Option<usize> {
    let lower = style.to_ascii_lowercase();
    let list = &lower[lower.find("mso-list:")?..];
    let level = &list[list.find("level")? + "level".len()..];
    let digits = level
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    digits
        .parse::<usize>()
        .ok()
        .map(|level| level.saturating_sub(1))
}

/// Outlook writes the marker of a list item as text (`·` and spaces): a line of text that
/// starts with one is an item, and an item loses the marker.
fn outlook_item(mut line: Line) -> Line {
    let Some(first) = line.runs.first() else {
        return line;
    };
    let text = first.text.trim_start().to_string();
    let indent = match line.kind {
        LineKind::Item { indent, .. } => indent,
        LineKind::Text => 0,
        _ => return line,
    };
    let nested = matches!(line.kind, LineKind::Item { .. });
    if let Some((ordered, _)) = item(&text, usize::from(nested)) {
        let marker_end = text.find(char::is_whitespace).unwrap_or(text.len());
        let rest = text[marker_end..].trim_start().to_string();
        line.kind = LineKind::Item { indent, ordered };
        if rest.is_empty() {
            line.runs.remove(0);
        } else {
            line.runs[0].text = rest;
        }
    }
    line
}

/// `text` with its character references (`&amp;`, `&#228;`, `&#xE4;`) replaced.
fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find('&') {
        out.push_str(&rest[..start]);
        rest = &rest[start..];
        let decoded = rest[1..]
            .find(';')
            .filter(|&end| end <= 10)
            .and_then(|end| Some((reference(&rest[1..=end])?, end + 2)));
        match decoded {
            Some((c, len)) => {
                out.push(c);
                rest = &rest[len..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn reference(name: &str) -> Option<char> {
    if let Some(number) = name.strip_prefix('#') {
        let code = match number.strip_prefix(['x', 'X']) {
            Some(hex) => u32::from_str_radix(hex, 16).ok()?,
            None => number.parse().ok()?,
        };
        return char::from_u32(code);
    }
    Some(match name {
        "amp" | "AMP" => '&',
        "lt" | "LT" => '<',
        "gt" | "GT" => '>',
        "quot" | "QUOT" => '"',
        "apos" => '\'',
        "nbsp" => ' ',
        "ndash" => '–',
        "mdash" => '—',
        "hellip" => '…',
        "lsquo" => '‘',
        "rsquo" => '’',
        "sbquo" => '‚',
        "ldquo" => '“',
        "rdquo" => '”',
        "bdquo" => '„',
        "bull" => '•',
        "middot" => '·',
        "copy" => '©',
        "reg" => '®',
        "trade" => '™',
        "euro" => '€',
        "auml" => 'ä',
        "ouml" => 'ö',
        "uuml" => 'ü',
        "Auml" => 'Ä',
        "Ouml" => 'Ö',
        "Uuml" => 'Ü',
        "szlig" => 'ß',
        "eacute" => 'é',
        "egrave" => 'è',
        "agrave" => 'à',
        _ => return None,
    })
}
