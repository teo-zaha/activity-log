use super::{ButtonArea, placement};

const MAXIMIZE: ButtonArea = ButtonArea {
    top: 0.0,
    right: 46.0,
    width: 46.0,
    height: 32.0,
};

#[test]
fn places_the_button_from_the_right_edge_in_physical_pixels() {
    assert_eq!(placement(MAXIMIZE, 1440, 1.0, 4), (1348, 4, 46, 28));
    assert_eq!(placement(MAXIMIZE, 2160, 1.5, 6), (2022, 6, 69, 42));
}

#[test]
fn keeps_a_button_below_the_resize_border() {
    let lower = ButtonArea {
        top: 8.0,
        ..MAXIMIZE
    };
    assert_eq!(placement(lower, 1000, 1.0, 4), (908, 8, 46, 32));
    assert_eq!(placement(MAXIMIZE, 1000, 1.0, 40).3, 0);
}
