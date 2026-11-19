use tauri_plugin_window_state::StateFlags;

use super::{BEFORE_SHOW, SAVED};

#[test]
fn saves_size_position_maximized_and_fullscreen_only() {
    assert_eq!(
        SAVED.bits(),
        (StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED | StateFlags::FULLSCREEN)
            .bits()
    );
    assert!(!SAVED.intersects(StateFlags::VISIBLE | StateFlags::DECORATIONS));
}

#[test]
fn restores_fullscreen_only_once_the_window_is_shown() {
    assert!(BEFORE_SHOW.contains(StateFlags::SIZE | StateFlags::POSITION | StateFlags::MAXIMIZED));
    assert!(!BEFORE_SHOW.contains(StateFlags::FULLSCREEN));
}
