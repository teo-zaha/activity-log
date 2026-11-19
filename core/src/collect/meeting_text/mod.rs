//! The readable part of a meeting description and its online-meeting link, shared by the
//! calendar feed and Outlook. Invitations arrive as plain text or HTML full of Outlook Safe
//! Links and a Teams block of dial-in details; what is left is the text the organizer wrote, in
//! the paragraphs, headings and lists they wrote it in (`structure`).

mod structure;

use std::ops::Range;

use reqwest::Url;

use crate::model::DescriptionBlock;

pub(crate) use structure::looks_like_html;
use structure::{Line, LineKind, Run, blocks, html_lines, plain_lines, plain_text};

#[cfg(test)]
mod tests;

/// Recurring series expand into many occurrences that each carry the description.
const MAX_DESCRIPTION_CHARS: usize = 4_000;
/// A line of at least this many underscores frames the Teams invitation block.
const MIN_DELIMITER_UNDERSCORES: usize = 20;
/// Text that only the Teams invitation block contains, in English and German invitations.
const TEAMS_MARKERS: [&str; 1] = ["Microsoft Teams"];

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct MeetingText {
    /// The description as plain text, for search and exports.
    pub description: Option<String>,
    /// The description as paragraphs, headings and lists, for rendering.
    pub blocks: Vec<DescriptionBlock>,
    pub join_url: Option<String>,
}

/// Cleans `description` and finds the join link: the first of `explicit` (properties made for
/// it, in order of preference) that is an https link, else the first online-meeting link in the
/// description. `html` is the description as HTML where the source has it besides the text
/// (`X-ALT-DESC`); a description that is HTML itself is read as HTML too.
pub(crate) fn meeting_text<'a>(
    description: Option<&str>,
    html: Option<&str>,
    explicit: impl IntoIterator<Item = &'a str>,
) -> MeetingText {
    let html = html
        .filter(|html| !html.trim().is_empty())
        .or(description.filter(|text| looks_like_html(text)));
    let lines = match html {
        Some(html) => html_lines(&unwrap_safe_links(html)),
        None => plain_lines(&unwrap_safe_links(
            &description
                .unwrap_or_default()
                .replace("\r\n", "\n")
                .replace('\r', "\n"),
        )),
    };
    let join_url = explicit
        .into_iter()
        .find_map(|candidate| https_url(&unwrap_safe_links(candidate.trim())))
        .or_else(|| {
            lines
                .iter()
                .flat_map(|line| &line.runs)
                .find_map(run_meeting_link)
        });
    let blocks = blocks(&clean(lines), MAX_DESCRIPTION_CHARS);
    MeetingText {
        description: Some(plain_text(&blocks)).filter(|text| !text.is_empty()),
        blocks,
        join_url,
    }
}

/// `raw` when it is an https link to a Teams, Zoom, Google Meet or Webex meeting, after
/// unwrapping a Safe Link.
pub(crate) fn meeting_link(raw: &str) -> Option<String> {
    let url = Url::parse(&unwrap_safe_links(trim_url(raw.trim()))).ok()?;
    if url.scheme() != "https" {
        return None;
    }
    let host = url.host_str()?.to_ascii_lowercase();
    let path = url.path().to_ascii_lowercase();
    let is_meeting = match host.as_str() {
        "teams.microsoft.com" => path.starts_with("/l/meetup-join/") || path.starts_with("/meet/"),
        "teams.live.com" => path.starts_with("/meet/"),
        "meet.google.com" => path.len() > 1,
        _ if host == "zoom.us" || host.ends_with(".zoom.us") => path.starts_with("/j/"),
        _ if host.ends_with(".webex.com") => path.len() > 1,
        _ => false,
    };
    is_meeting.then(|| url.to_string())
}

/// The online-meeting link of a run: where it links to, or one written in its text (HTML
/// links only what is in an anchor).
fn run_meeting_link(run: &Run) -> Option<String> {
    match run.href.as_deref() {
        Some(href) => meeting_link(href),
        None => url_spans(&run.text)
            .into_iter()
            .find_map(|span| meeting_link(&run.text[span])),
    }
}

fn https_url(raw: &str) -> Option<String> {
    Url::parse(trim_url(raw))
        .ok()
        .filter(|url| url.scheme() == "https" && url.host_str().is_some())
        .map(|url| url.to_string())
}

/// Replaces every Outlook Safe Link with the link it wraps.
fn unwrap_safe_links(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut copied = 0;
    for span in url_spans(text) {
        if let Some(target) = safe_link_target(&text[span.clone()]) {
            out.push_str(&text[copied..span.start]);
            out.push_str(&target);
            copied = span.end;
        }
    }
    out.push_str(&text[copied..]);
    out
}

/// `https://eur03.safelinks.protection.outlook.com/?url=…&data=…`, also with a path such as
/// `/ap/t-…/`: the `url` parameter, when it is an http(s) link.
fn safe_link_target(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    let host = url.host_str()?.to_ascii_lowercase();
    if !host.ends_with(".safelinks.protection.outlook.com") {
        return None;
    }
    let (_, target) = url.query_pairs().find(|(name, _)| name == "url")?;
    let target = Url::parse(&target).ok()?;
    matches!(target.scheme(), "http" | "https").then(|| target.to_string())
}

/// Where `http://` and `https://` links are in `text`. A link ends at white space, quotes,
/// angle or square brackets; trailing punctuation is not part of it.
fn url_spans(text: &str) -> Vec<Range<usize>> {
    // ASCII lowercasing keeps every byte offset.
    let lower = text.to_ascii_lowercase();
    let mut spans = Vec::new();
    let mut from = 0;
    while let Some((start, scheme_len)) = ["https://", "http://"]
        .iter()
        .filter_map(|scheme| Some((from + lower[from..].find(scheme)?, scheme.len())))
        .min()
    {
        let len = text[start..]
            .find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '[' | ']'))
            .unwrap_or(text.len() - start);
        let end = start + trim_url(&text[start..start + len]).len();
        if end > start + scheme_len {
            spans.push(start..end);
        }
        from = start + len.max(1);
    }
    spans
}

fn trim_url(raw: &str) -> &str {
    raw.trim_end_matches(['>', ')', ']', '.', ',', ';', '!', '?'])
}

/// `lines` without the invitation block and the underscore lines left.
fn clean(lines: Vec<Line>) -> Vec<Line> {
    drop_invitation_blocks(lines)
        .into_iter()
        .map(|line| {
            if is_delimiter(&line.text()) {
                Line::blank()
            } else {
                line
            }
        })
        .collect()
}

fn is_delimiter(line: &str) -> bool {
    let line = line.trim();
    line.len() >= MIN_DELIMITER_UNDERSCORES && line.chars().all(|c| c == '_')
}

/// Removes each block between two underscore lines that holds a meeting invitation: a meeting
/// link or the Teams markers. Other text between underscore lines stays.
fn drop_invitation_blocks(lines: Vec<Line>) -> Vec<Line> {
    let delimiters = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| line.kind != LineKind::Blank && is_delimiter(&line.text()))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    let mut dropped = vec![false; lines.len()];
    let mut pair = 0;
    while let [open, close, ..] = delimiters[pair..] {
        let is_invitation = lines[open..=close].iter().any(|line| {
            let text = line.text();
            TEAMS_MARKERS.iter().any(|marker| text.contains(marker))
                || line.runs.iter().any(|run| run_meeting_link(run).is_some())
        });
        if is_invitation {
            dropped[open..=close].fill(true);
            pair += 2;
        } else {
            pair += 1;
        }
    }
    lines
        .into_iter()
        .zip(dropped)
        .filter_map(|(line, dropped)| (!dropped).then_some(line))
        .collect()
}
