use tauri_plugin_window_state::StateFlags;

use super::{BEFORE_SHOW, SAVED};

#[test]
fn saves_where_the_window_was_but_never_its_visibility_or_decorations() {
    assert!(SAVED.contains(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED));
    assert!(SAVED.contains(StateFlags::FULLSCREEN));
    // The window starts hidden until its splash is styled; the platform's config decides its
    // frame.
    assert!(!SAVED.intersects(StateFlags::VISIBLE | StateFlags::DECORATIONS));
}

#[test]
fn restores_fullscreen_only_once_the_window_is_shown() {
    assert!(BEFORE_SHOW.contains(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED));
    assert!(!BEFORE_SHOW.contains(StateFlags::FULLSCREEN));
}
