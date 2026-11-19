use super::{MAX_DESCRIPTION_CHARS, MeetingText, meeting_link, meeting_text};

const RULE: &str =
    "________________________________________________________________________________";
const TEAMS_JOIN: &str =
    "https://teams.microsoft.com/l/meetup-join/19%3ameeting_example%40thread.v2/0?context=%7b%7d";

fn clean(description: &str) -> MeetingText {
    meeting_text(Some(description), None, [])
}

/// How Safe Links wrap `target`, in the plain form or with the `/ap/…/` path.
fn safe_link(target: &str, with_path: bool) -> String {
    let path = if with_path { "/ap/t-1234abcd/" } else { "/" };
    format!(
        "https://eur03.safelinks.protection.outlook.com{path}?url={}&data=05%7C02%7Cx&reserved=0",
        urlencoding::encode(target)
    )
}

fn english_invitation(join: &str) -> String {
    format!(
        "{RULE}\n\
Microsoft Teams meeting\n\
Join on your computer, mobile app or room device\n\
Click here to join the meeting<{join}>\n\
Meeting ID: 123 456 789 012\n\
Passcode: abc123\n\
Download Teams<https://www.microsoft.com/microsoft-teams/download-app> | Join on the web<https://www.microsoft.com/microsoft-teams/join-a-meeting>\n\
Dial in by phone\n\
+49 30 1234567,,111222333# Germany, Berlin\n\
Find a local number<https://dialin.teams.microsoft.com/example>\n\
Phone conference ID: 111 222 333#\n\
Need help?<https://aka.ms/JoinTeamsMeeting> | Meeting options<https://teams.microsoft.com/meetingOptions/>\n\
{RULE}\n"
    )
}

fn german_invitation(join: &str) -> String {
    format!(
        "{RULE}\n\
Microsoft Teams-Besprechung\n\
Treten Sie über Ihren Computer, Ihre mobile App oder Ihr Raumgerät bei\n\
Klicken Sie hier, um an der Besprechung teilzunehmen<{join}>\n\
Besprechungs-ID: 123 456 789 012\n\
Kennung: abc123\n\
Per Telefon einwählen\n\
+49 30 1234567,,111222333# Deutschland, Berlin\n\
Benötigen Sie Hilfe?<https://aka.ms/JoinTeamsMeeting>\n\
Für Organisatoren: Besprechungsoptionen<https://teams.microsoft.com/meetingOptions/>\n\
Datenschutz<https://example.org/datenschutz>\n\
{RULE}\n"
    )
}

#[test]
fn unwraps_safe_links_in_both_forms() {
    let target = "https://example.org/agenda?id=7&lang=de";
    for with_path in [false, true] {
        let text = clean(&format!("Agenda: {}", safe_link(target, with_path)));
        assert_eq!(
            text.description.as_deref(),
            Some(format!("Agenda: {target}").as_str()),
            "with_path: {with_path}"
        );
    }
}

#[test]
fn keeps_a_safe_link_that_does_not_wrap_a_web_link() {
    let wrapped = safe_link("javascript:alert(1)", false);
    let text = clean(&format!("See {wrapped}"));
    assert_eq!(text.description, Some(format!("See {wrapped}")));
    let other_host = "https://safelinks.example.com/?url=https%3A%2F%2Fexample.org%2F";
    assert_eq!(
        clean(other_host).description.as_deref(),
        Some(other_host),
        "only Outlook's Safe Links hosts are unwrapped"
    );
}

#[test]
fn removes_the_english_teams_block_and_keeps_its_join_link() {
    let description = format!(
        "Hi all,\r\n\r\nlet's review the release plan.\r\n\r\n{}",
        english_invitation(&safe_link(TEAMS_JOIN, true)).replace('\n', "\r\n")
    );
    let text = clean(&description);
    assert_eq!(
        text.description.as_deref(),
        Some("Hi all,\n\nlet's review the release plan.")
    );
    assert_eq!(text.join_url.as_deref(), Some(TEAMS_JOIN));
}

#[test]
fn removes_the_german_teams_block_with_the_short_meeting_link() {
    let join = "https://teams.microsoft.com/meet/38512345678901?p=AbCdEf123";
    let text = clean(&format!(
        "Abstimmung zum Projektstand\n\n{}",
        german_invitation(&safe_link(join, false))
    ));
    assert_eq!(
        text.description.as_deref(),
        Some("Abstimmung zum Projektstand")
    );
    assert_eq!(text.join_url.as_deref(), Some(join));
}

#[test]
fn a_description_that_is_only_the_invitation_becomes_none() {
    let text = clean(&format!("\n\n{}\n\n", english_invitation(TEAMS_JOIN)));
    assert_eq!(text.description, None);
    assert_eq!(text.join_url.as_deref(), Some(TEAMS_JOIN));
    assert_eq!(meeting_text(None, None, []), MeetingText::default());
    assert_eq!(clean(" \n\u{a0}\n\t").description, None);
}

#[test]
fn keeps_user_text_between_underscore_lines_as_paragraphs() {
    let description = format!("Notes\n{RULE}\nBring the printed agenda.\n{RULE}\nThanks");
    assert_eq!(
        clean(&description).description.as_deref(),
        Some("Notes\n\nBring the printed agenda.\n\nThanks")
    );
}

#[test]
fn an_extra_underscore_line_before_the_invitation_breaks_the_text() {
    let description = format!(
        "Notes\n{RULE}\nBring the printed agenda.\n\n{}",
        english_invitation(TEAMS_JOIN)
    );
    assert_eq!(
        clean(&description).description.as_deref(),
        Some("Notes\n\nBring the printed agenda.")
    );
}

#[test]
fn finds_each_kind_of_meeting_link() {
    for link in [
        TEAMS_JOIN,
        "https://teams.microsoft.com/meet/12345678?p=abc",
        "https://teams.live.com/meet/9876543210",
        "https://us02web.zoom.us/j/81234567890?pwd=abc",
        "https://zoom.us/j/81234567890",
        "https://meet.google.com/abc-defg-hij",
        "https://example.webex.com/meet/someone",
    ] {
        assert_eq!(
            clean(&format!("Join: {link}")).join_url.as_deref(),
            Some(link),
            "{link}"
        );
    }
}

#[test]
fn finds_links_in_brackets_and_strips_trailing_punctuation() {
    for (text, expected) in [
        (
            "Join here<https://meet.google.com/abc-defg-hij>",
            "https://meet.google.com/abc-defg-hij",
        ),
        ("(https://zoom.us/j/123).", "https://zoom.us/j/123"),
        (
            "Link: https://zoom.us/j/123, see you",
            "https://zoom.us/j/123",
        ),
        (
            "[https://meet.google.com/abc-defg-hij]",
            "https://meet.google.com/abc-defg-hij",
        ),
    ] {
        assert_eq!(clean(text).join_url.as_deref(), Some(expected), "{text}");
    }
}

#[test]
fn other_links_are_no_meeting_links() {
    for text in [
        "https://example.org/meeting",
        "https://teams.microsoft.com/meetingOptions/",
        "https://zoom.us/pricing",
        "http://meet.google.com/abc-defg-hij",
        "https://meet.google.com/",
        "ftp://zoom.us/j/1",
    ] {
        assert_eq!(clean(text).join_url, None, "{text}");
    }
    assert_eq!(meeting_link("not a link"), None);
}

#[test]
fn an_explicit_join_link_wins_over_the_description() {
    let explicit = "https://teams.microsoft.com/l/meetup-join/explicit";
    let text = meeting_text(
        Some("Fallback https://zoom.us/j/1"),
        None,
        ["", "javascript:void(0)", explicit],
    );
    assert_eq!(text.join_url.as_deref(), Some(explicit));

    let wrapped = safe_link("https://meet.google.com/abc-defg-hij", true);
    let text = meeting_text(None, None, [wrapped.as_str()]);
    assert_eq!(
        text.join_url.as_deref(),
        Some("https://meet.google.com/abc-defg-hij"),
        "an explicit Safe Link is unwrapped"
    );

    let text = meeting_text(
        Some("Fallback https://zoom.us/j/1"),
        None,
        ["http://insecure.example"],
    );
    assert_eq!(text.join_url.as_deref(), Some("https://zoom.us/j/1"));
}

#[test]
fn reads_outlooks_bracketed_images_as_image_blocks() {
    use crate::model::{DescriptionBlock, TextSegment};

    let text = clean(
        "[https://example.org/files/Team%20Photo.JPG]\nHello\n[Company Logo]\n  [cid:image001.png@01DA0000.00000000]\n\
[https://example.org/agenda]\n[http://example.org/logo.png]",
    );
    assert_eq!(
        text.blocks,
        [
            DescriptionBlock::Image {
                src: Some("https://example.org/files/Team%20Photo.JPG".to_string()),
                alt: None,
            },
            DescriptionBlock::Paragraph {
                segments: vec![TextSegment::text("Hello")]
            },
            DescriptionBlock::Image {
                src: None,
                alt: Some("Company Logo".to_string()),
            },
            DescriptionBlock::Image {
                src: None,
                alt: None
            },
            DescriptionBlock::Paragraph {
                segments: vec![
                    TextSegment::link("https://example.org/agenda", "https://example.org/agenda"),
                    TextSegment::text("\n"),
                    TextSegment::link("http://example.org/logo.png", "http://example.org/logo.png"),
                ]
            },
        ],
        "only https links to image files become images; other links stay links"
    );
    assert_eq!(
        text.description.as_deref(),
        Some("Hello\n\nCompany Logo\n\nhttps://example.org/agenda\nhttp://example.org/logo.png"),
        "an image is its alt text in plain text, never its link"
    );
}

#[test]
fn keeps_text_in_brackets_that_is_no_image() {
    for line in [
        "[SCP] Sprint Review",
        "Please read [the notes] first",
        "[see https://example.org]",
        "[mailto:jane@example.org]",
        "[x]",
        "[ ]",
        "[42]",
        "[javascript:alert(1)]",
        "[https://example.org/a.png and more]",
        "[Company Logo] and text",
    ] {
        let text = clean(line);
        assert!(
            !text
                .blocks
                .iter()
                .any(|block| matches!(block, crate::model::DescriptionBlock::Image { .. })),
            "{line}"
        );
        assert_eq!(text.description.as_deref(), Some(line), "{line}");
    }
    let long = format!("[{}]", "word ".repeat(20).trim());
    assert_eq!(clean(&long).description, Some(long));
}

#[test]
fn a_description_of_only_an_image_has_blocks_and_no_text() {
    let text = clean("[https://example.org/banner.png]");
    assert_eq!(text.description, None);
    assert_eq!(text.blocks.len(), 1);
}

#[test]
fn collapses_blank_lines_and_a_link_used_as_its_own_label() {
    let text = clean(&format!(
        "One  \n\n\n\n\nTwo\n\n{}<{}>\nText<https://example.org/b>",
        safe_link("https://example.org/a", false),
        safe_link("https://example.org/a", true),
    ));
    assert_eq!(
        text.description.as_deref(),
        Some("One\n\nTwo\n\nhttps://example.org/a\nText <https://example.org/b>")
    );
}

#[test]
fn caps_a_long_description_on_a_character_boundary() {
    let text = clean(&"ä".repeat(MAX_DESCRIPTION_CHARS + 10));
    let description = text.description.unwrap();
    assert_eq!(description.chars().count(), MAX_DESCRIPTION_CHARS);
    assert!(description.ends_with("ä…"));

    let exact = "x".repeat(MAX_DESCRIPTION_CHARS);
    assert_eq!(clean(&exact).description, Some(exact));
}

#[test]
fn reads_html_as_structure_and_drops_scripts() {
    use crate::model::{DescriptionBlock, TextSegment};

    let text = clean("<b>bold</b> &amp; <script>alert(1)</script>more");
    assert_eq!(text.description.as_deref(), Some("bold & more"));
    assert_eq!(
        text.blocks,
        [DescriptionBlock::Paragraph {
            segments: vec![
                TextSegment::Text {
                    text: "bold".to_string(),
                    strong: true,
                    emphasis: false,
                },
                TextSegment::text(" & more"),
            ]
        }]
    );
    // Text that only mentions a tag stays text.
    assert_eq!(
        clean("Use x < y and <tag> as is").description.as_deref(),
        Some("Use x < y and <tag> as is")
    );
}

#[test]
fn prefers_the_html_version_of_the_description() {
    let text = meeting_text(
        Some("Agenda\n* One"),
        Some("<p>Agenda</p><ul><li>One</li><li>Two</li></ul>"),
        [],
    );
    assert_eq!(text.description.as_deref(), Some("Agenda\n\n• One\n• Two"));
}

#[test]
fn finds_the_join_link_and_drops_the_invitation_in_html() {
    let html = format!(
        "<div>Weekly sync</div><div>{RULE}</div><div>Microsoft Teams meeting</div>\
<div><a href=\"{}\">Join the meeting now</a></div><div>{RULE}</div>",
        safe_link(TEAMS_JOIN, false)
    );
    let text = clean(&html);
    assert_eq!(text.description.as_deref(), Some("Weekly sync"));
    assert_eq!(text.join_url.as_deref(), Some(TEAMS_JOIN));
}

#[test]
fn splits_the_description_into_text_and_its_web_links() {
    use crate::model::{DescriptionBlock, TextSegment};

    let text = clean(
        "Agenda: https://example.org/agenda.\nNot a link: http:// or https://\nmailto:a@b.c\n\
Slides: https://example.org/a?b=1, javascript:alert(1)",
    );

    assert_eq!(
        text.blocks,
        [DescriptionBlock::Paragraph {
            segments: vec![
                TextSegment::text("Agenda: "),
                TextSegment::link("https://example.org/agenda", "https://example.org/agenda"),
                TextSegment::text(".\nNot a link: http:// or https://\nmailto:a@b.c\nSlides: "),
                TextSegment::link("https://example.org/a?b=1", "https://example.org/a?b=1"),
                TextSegment::text(", javascript:alert(1)"),
            ]
        }]
    );
    assert!(clean("").blocks.is_empty());
}

#[test]
fn the_cap_never_cuts_a_link_in_two() {
    use crate::model::{DescriptionBlock, TextSegment};

    let link = format!("https://example.org/{}", "path/".repeat(10));
    let filler = "x".repeat(MAX_DESCRIPTION_CHARS - 20);
    let text = clean(&format!("{filler} {link} and more"));

    let description = text.description.unwrap();
    assert_eq!(description, format!("{filler}…"));
    let links = |blocks: &[DescriptionBlock]| {
        blocks
            .iter()
            .flat_map(|block| match block {
                DescriptionBlock::Paragraph { segments } => segments.clone(),
                _ => Vec::new(),
            })
            .filter_map(|segment| match segment {
                TextSegment::Link { text, .. } => Some(text),
                TextSegment::Text { .. } => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(links(&text.blocks).is_empty());
    // A link that ends before the cut stays whole and clickable.
    let short = clean(&format!("{} {link}", "x".repeat(10)));
    assert_eq!(links(&short.blocks), [link]);
}

#[test]
fn reads_a_description_that_is_only_an_html_image() {
    use crate::model::DescriptionBlock;

    let img = "<img src=\"https://example.org/banner.png\" alt=\"Banner\">";
    let banner = [DescriptionBlock::Image {
        src: Some("https://example.org/banner.png".to_string()),
        alt: Some("Banner".to_string()),
    }];
    assert!(
        super::looks_like_html(img),
        "the calendar reads X-ALT-DESC only when it is HTML"
    );
    assert_eq!(clean(img).blocks, banner);
    assert_eq!(meeting_text(Some("[Banner]"), Some(img), []).blocks, banner);
    assert_eq!(clean(img).description.as_deref(), Some("Banner"));
}

/// A meeting link written as text in an HTML paragraph is the join link, as in plain text.
#[test]
fn a_meeting_link_written_as_text_in_html_is_the_join_link() {
    let html = meeting_text(
        Some("<p>Join here: https://meet.google.com/abc-defg-hij</p><p>Agenda</p>"),
        None,
        [],
    );
    let plain = clean("Join here: https://meet.google.com/abc-defg-hij\n\nAgenda");

    assert_eq!(
        html.join_url.as_deref(),
        Some("https://meet.google.com/abc-defg-hij")
    );
    assert_eq!(html.join_url, plain.join_url);
}

#[test]
fn only_real_tags_make_a_text_html() {
    use super::looks_like_html;

    // Plain text that mentions something in angle brackets, or compares.
    for plain in [
        "Release <bravo> notes",
        "Bring <pizza> and <icecream>",
        "if a<b and c>d then",
        "see <a> above",
    ] {
        assert!(!looks_like_html(plain), "{plain}");
    }
    for html in [
        "<h2>Agenda</h2>",
        "Line one<br>Line two",
        "<p>Text</p>",
        "<div class=\"x\">Text</div>",
        "<a href=\"https://example.com\">Link</a>",
        "<img src=\"https://example.com/logo.png\">",
        "<UL><LI>One</LI></UL>",
    ] {
        assert!(looks_like_html(html), "{html}");
    }

    let heading = clean("<h2>Agenda</h2>");
    assert_eq!(heading.description.as_deref(), Some("Agenda"));
    let bracketed = clean("Release <bravo> notes");
    assert_eq!(
        bracketed.description.as_deref(),
        Some("Release <bravo> notes")
    );
}

#[test]
fn a_tag_name_ends_only_where_a_tag_does() {
    // An address in angle brackets is no `<br>`.
    let address = clean("Write <br@example.com> here");
    assert_eq!(
        address.description.as_deref(),
        Some("Write <br@example.com> here")
    );
    // Paragraphs need not be closed, and a closing tag may have space before `>`.
    for html in ["<p>Agenda<p>Notes", "<h2>Agenda</h2 >"] {
        assert!(super::looks_like_html(html), "{html}");
        assert!(!clean(html).description.unwrap().contains('<'), "{html}");
    }
}
