use super::*;

#[test]
fn parse_single_zoom() {
    let r = ZoomRange::parse("5").unwrap();
    assert_eq!(r.min, 5);
    assert_eq!(r.max, 5);
}

#[test]
fn parse_range() {
    let r = ZoomRange::parse("2..8").unwrap();
    assert_eq!(r.min, 2);
    assert_eq!(r.max, 8);
}

#[test]
fn parse_range_same() {
    let r = ZoomRange::parse("3..3").unwrap();
    assert_eq!(r.min, 3);
    assert_eq!(r.max, 3);
}

#[test]
fn parse_invalid_range_order() {
    assert!(ZoomRange::parse("8..2").is_err());
}

#[test]
fn parse_invalid_not_a_number() {
    assert!(ZoomRange::parse("abc").is_err());
}

#[test]
fn parse_edge_zoom_zero() {
    let r = ZoomRange::parse("0").unwrap();
    assert_eq!(r.min, 0);
    assert_eq!(r.max, 0);
}
