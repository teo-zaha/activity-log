use crate::model::{DescriptionBlock, ListItem, TextSegment};

use super::{blocks, html_lines, plain_lines, plain_text};

fn plain(text: &str) -> Vec<DescriptionBlock> {
    blocks(&plain_lines(text), 4_000)
}

fn html(text: &str) -> Vec<DescriptionBlock> {
    blocks(&html_lines(text), 4_000)
}

fn paragraph(segments: Vec<TextSegment>) -> DescriptionBlock {
    DescriptionBlock::Paragraph { segments }
}

fn list(ordered: bool, items: &[(u8, &str)]) -> DescriptionBlock {
    DescriptionBlock::List {
        ordered,
        items: items
            .iter()
            .map(|(depth, text)| ListItem {
                depth: *depth,
                segments: vec![TextSegment::text(*text)],
            })
            .collect(),
    }
}

fn strong(text: &str) -> TextSegment {
    TextSegment::Text {
        text: text.to_string(),
        strong: true,
        emphasis: false,
    }
}

fn emphasis(text: &str) -> TextSegment {
    TextSegment::Text {
        text: text.to_string(),
        strong: false,
        emphasis: true,
    }
}

#[test]
fn reads_paragraphs_and_the_lists_outlook_writes_as_text() {
    let text = "Participants: all teams\n\
An end-of-sprint review\nacross teams.\n\n\
Agenda:\n\n  *   Team A shows its work\n  *   Team B shows its work\n     *   in detail\n\n\
Steps:\n  1.  Prepare\n  2.  Present\n";

    assert_eq!(
        plain(text),
        [
            paragraph(vec![TextSegment::text(
                "Participants: all teams\nAn end-of-sprint review\nacross teams."
            )]),
            paragraph(vec![TextSegment::text("Agenda:")]),
            list(
                false,
                &[
                    (0, "Team A shows its work"),
                    (0, "Team B shows its work"),
                    (1, "in detail")
                ]
            ),
            paragraph(vec![TextSegment::text("Steps:")]),
            list(true, &[(0, "Prepare"), (0, "Present")]),
        ]
    );
}

#[test]
fn reads_dashes_bullets_and_outlooks_nested_markers() {
    assert_eq!(
        plain("- one\n• two\n    o   nested\n        §   deeper\no not an item"),
        [
            list(
                false,
                &[(0, "one"), (0, "two"), (1, "nested"), (2, "deeper")]
            ),
            paragraph(vec![TextSegment::text("o not an item")]),
        ]
    );
    // A marker needs a space after it, and a number at most three digits and a dot.
    assert_eq!(
        plain("-5 degrees\n*not a list*\n2026. was a year"),
        [paragraph(vec![
            TextSegment::text("-5 degrees\n"),
            emphasis("not a list"),
            TextSegment::text("\n2026. was a year"),
        ])]
    );
}

#[test]
fn reads_markdown_headings_emphasis_and_links() {
    assert_eq!(
        plain(
            "# Plan\nThis is **important** and *new*: [the doc](https://example.org/doc).\n2 * 3 * 4"
        ),
        [
            DescriptionBlock::Heading {
                segments: vec![TextSegment::text("Plan")]
            },
            paragraph(vec![
                TextSegment::text("This is "),
                strong("important"),
                TextSegment::text(" and "),
                emphasis("new"),
                TextSegment::text(": "),
                TextSegment::link("the doc", "https://example.org/doc"),
                TextSegment::text(".\n2 * 3 * 4"),
            ]),
        ]
    );
}

#[test]
fn names_outlooks_links_by_the_text_before_them() {
    assert_eq!(
        plain(
            "Agenda: Planning board<https://example.org/board> and notes\n\
https://example.org/x<https://example.org/x>\n\
Ask Jane Doe<mailto:jane@example.org> for access\n\
This is a very long sentence that is no label of a link<https://example.org/long>"
        ),
        [paragraph(vec![
            TextSegment::text("Agenda: "),
            TextSegment::link("Planning board", "https://example.org/board"),
            TextSegment::text(" and notes\n"),
            TextSegment::link("https://example.org/x", "https://example.org/x"),
            TextSegment::text(
                "\nAsk Jane Doe for access\nThis is a very long sentence that is no label of a link"
            ),
            TextSegment::link("example.org", "https://example.org/long"),
        ])]
    );
}

/// Every named reference HTML has is read, not only the common ones.
#[test]
fn reads_every_named_character_reference() {
    assert_eq!(
        html("<p>Le&oacute;n &laquo;Fran&ccedil;ois&raquo; &Aring;s &rarr; &frac12; &amp;c</p>"),
        [paragraph(vec![TextSegment::text(
            "León «François» Ås → ½ &c"
        )])]
    );
}

#[test]
fn reads_html_paragraphs_breaks_lists_and_emphasis() {
    assert_eq!(
        html(
            "<html><head><style>p { color: red }</style></head><body>\
<h2>Sprint review</h2><p>Hello <b>all</b>,<br>see <a href=\"https://example.org/a\">the <i>board</i></a>.</p>\
<ol><li>First</li><li>Second<ul><li>Detail</li></ul></li></ol>\
<p>Caf&eacute; &amp; more&nbsp;&#8211; &#x2713;</p><a href=\"javascript:alert(1)\">no link</a></body></html>"
        ),
        [
            DescriptionBlock::Heading {
                segments: vec![TextSegment::text("Sprint review")]
            },
            paragraph(vec![
                TextSegment::text("Hello "),
                strong("all"),
                TextSegment::text(",\nsee "),
                TextSegment::link("the board", "https://example.org/a"),
                TextSegment::text("."),
            ]),
            DescriptionBlock::List {
                ordered: true,
                items: vec![
                    ListItem {
                        depth: 0,
                        segments: vec![TextSegment::text("First")]
                    },
                    ListItem {
                        depth: 0,
                        segments: vec![TextSegment::text("Second")]
                    },
                    ListItem {
                        depth: 1,
                        segments: vec![TextSegment::text("Detail")]
                    },
                ]
            },
            paragraph(vec![TextSegment::text("Café & more – ✓")]),
            paragraph(vec![TextSegment::text("no link")]),
        ]
    );
}

#[test]
fn reads_outlooks_html_lines_and_list_paragraphs() {
    let html_text = "<p class=MsoNormal>Hi all,<o:p></o:p></p>\
<p class=MsoNormal>please prepare:<o:p></o:p></p>\
<p class=MsoNormal><o:p>&nbsp;</o:p></p>\
<p class=MsoListParagraph style='text-indent:-18.0pt;mso-list:l0 level1 lfo1'>\
<![if !supportLists]><span>·<span>&nbsp;&nbsp;&nbsp;</span></span><![endif]>Slides<o:p></o:p></p>\
<p class=MsoListParagraph style='mso-list:l0 level2 lfo1'><![if !supportLists]>\
<span>o<span>&nbsp;&nbsp;</span></span><![endif]>Demo<o:p></o:p></p>";

    assert_eq!(
        html(html_text),
        [
            paragraph(vec![TextSegment::text("Hi all,\nplease prepare:")]),
            list(false, &[(0, "Slides"), (1, "Demo")]),
        ]
    );
}

#[test]
fn writes_blocks_as_plain_text() {
    let blocks = plain("# Plan\nSee the board<https://example.org/b>.\n\n1. One\n   - sub\n2. Two");
    assert_eq!(
        plain_text(&blocks),
        "Plan\n\nSee the board <https://example.org/b>.\n\n1. One\n  • sub\n2. Two"
    );
}

#[test]
fn caps_the_text_across_blocks() {
    // 11 + 2 between the blocks + 3, and two left for `t…`.
    let blocks = blocks(&plain_lines("Hello world\n\n* one\n* two\n* three"), 18);
    assert_eq!(
        blocks,
        [
            paragraph(vec![TextSegment::text("Hello world")]),
            list(false, &[(0, "one"), (0, "t…")]),
        ]
    );
}

fn image(src: Option<&str>, alt: Option<&str>) -> DescriptionBlock {
    DescriptionBlock::Image {
        src: src.map(str::to_string),
        alt: alt.map(str::to_string),
    }
}

#[test]
fn reads_html_images_without_their_links_and_tracking_pixels() {
    assert_eq!(
        html(
            "<p>Hello <img src=\"https://example.org/team.jpg\" alt=\" The  team \"> all</p>\
<a href=\"https://example.org/home\"><img src='https://example.org/logo'></a>\
<p><img src=\"cid:image001.png@01DA0000\" alt=\"Company Logo\" width=\"120\"></p>\
<img src=\"https://tracker.example.org/open.gif\" width=\"1\" height=\"1\">\
<img src=\"https://tracker.example.org/p.gif\" height=\"0px\">\
<img src=\"data:image/png;base64,AAAA\"><img src=\"javascript:alert(1)\" alt=\"x\">\
<img src=\"http://example.org/plain.png\"><img src=\"file:///etc/passwd\">\
<ul><li><img src=\"https://example.org/icon.png\">Item</li></ul>"
        ),
        [
            paragraph(vec![TextSegment::text("Hello")]),
            image(Some("https://example.org/team.jpg"), Some("The team")),
            paragraph(vec![TextSegment::text("all")]),
            image(Some("https://example.org/logo"), None),
            image(None, Some("Company Logo")),
            image(None, Some("x")),
            list(false, &[(0, "Item")]),
            image(Some("https://example.org/icon.png"), None),
        ]
    );
}

#[test]
fn an_image_counts_towards_the_cap_and_is_its_alt_text() {
    let lines = vec![
        super::Line::image(None, Some("Logo".to_string())),
        super::Line::image(Some("https://example.org/a.png".to_string()), None),
        super::Line::image(None, Some("Too long".to_string())),
    ];
    // `Logo`, 2 between blocks, 1 for the image; the next would need 2 + 8.
    let capped = blocks(&lines, 9);
    assert_eq!(
        capped,
        [
            image(None, Some("Logo")),
            image(Some("https://example.org/a.png"), None),
        ],
        "an image costs its alt text, at least one character, and goes as a whole"
    );
    assert_eq!(plain_text(&capped), "Logo");
}

#[test]
fn skips_images_hidden_or_sized_away_in_their_style() {
    for img in [
        "<img src=\"https://example.org/p.gif\" style=\"width:1px;height:1px\">",
        "<img src=\"https://example.org/p.gif\" style=\"WIDTH: 0 !important\">",
        "<img src=\"https://example.org/p.gif\" style=\"max-height:1px\">",
        "<img src=\"https://example.org/p.gif\" style=\"display: none\">",
        "<img src=\"https://example.org/p.gif\" style=\"visibility:hidden\">",
        "<img src=\"https://example.org/p.gif\" style=\"opacity:0\">",
        "<img src=\"https://example.org/p.gif\" hidden>",
        "<img src=\"https://example.org/p.gif\" width=\"0.5\">",
    ] {
        assert_eq!(
            html(&format!("<p>Hi</p>{img}")),
            [paragraph(vec![TextSegment::text("Hi")])],
            "{img}"
        );
    }
    assert_eq!(
        html("<img src=\"https://example.org/a.png\" style=\"width:100%;height:auto\">"),
        [image(Some("https://example.org/a.png"), None)]
    );
}

#[test]
fn keeps_an_image_source_as_written_and_only_absolute_https() {
    assert_eq!(
        html(
            "<img src=\"https://example.org/a.png?key=abc!\" alt=\"A\">\
<img src=\"https:images/banner\" alt=\"B\"><img src=\"https://example.org/b.png?k=9&AMP;q=7\">"
        ),
        [
            image(Some("https://example.org/a.png?key=abc!"), Some("A")),
            image(None, Some("B")),
            image(Some("https://example.org/b.png?k=9&q=7"), None),
        ]
    );
}

#[test]
fn an_image_in_a_list_comes_after_it() {
    assert_eq!(
        html(
            "<ol><li>Prepare <img src=\"https://example.org/a.png\"> draft</li><li>Send</li></ol>\
<ul><li><img src=\"https://example.org/b.png\"></li></ul><p>After</p>"
        ),
        [
            list(true, &[(0, "Prepare draft"), (0, "Send")]),
            image(Some("https://example.org/a.png"), None),
            image(Some("https://example.org/b.png"), None),
            paragraph(vec![TextSegment::text("After")]),
        ]
    );
}

#[test]
fn keeps_an_empty_alt_text_and_drops_a_decorative_image_without_its_data() {
    assert_eq!(
        html("<img src=\"https://example.org/rule.png\" alt=\"\"><img src=\"cid:x\" alt=\"\">"),
        [image(Some("https://example.org/rule.png"), Some(""))]
    );
}

#[test]
fn a_custom_element_named_like_an_image_is_no_image() {
    assert_eq!(
        html("<p><img-caption src=\"https://example.org/a.png\">Caption</img-caption></p>"),
        [paragraph(vec![TextSegment::text("Caption")])]
    );
}

#[test]
fn reads_brackets_from_the_plain_line_as_written() {
    assert_eq!(
        html(
            "<a href=\"https://example.org/help\">[https://example.org/sample.jpg]</a><br>\
<a href=\"https://example.org/dl\">[Download]</a>"
        ),
        [paragraph(vec![
            TextSegment::link(
                "[https://example.org/sample.jpg]",
                "https://example.org/help"
            ),
            TextSegment::text("\n"),
            TextSegment::link("[Download]", "https://example.org/dl"),
        ])],
        "brackets in HTML are text"
    );
    assert!(
        !plain("[[Minutes]](https://example.org/minutes)")
            .iter()
            .any(|block| matches!(block, DescriptionBlock::Image { .. })),
        "a Markdown link in brackets is no image"
    );
    assert_eq!(
        plain("[https://[2001:db8::7]/map.png]\n[Logo:]"),
        [
            image(Some("https://[2001:db8::7]/map.png"), None),
            image(None, Some("Logo:")),
        ]
    );
}

#[test]
fn the_cap_counts_the_text_between_blocks() {
    let lines = plain_lines(&"[AB]\n".repeat(2_000));
    let capped = blocks(&lines, 4_000);
    assert!(plain_text(&capped).chars().count() <= 4_000);
}
