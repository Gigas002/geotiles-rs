use libgeotiles::Crs;

use super::resolve_crs;

#[test]
fn explicit_crs_wins() {
    assert_eq!(
        resolve_crs(Some(Crs::WebMercator), Some(4326)).unwrap(),
        Crs::WebMercator
    );
    assert_eq!(resolve_crs(Some(Crs::Wgs84), None).unwrap(), Crs::Wgs84);
}

#[test]
fn crs_taken_from_raster() {
    assert_eq!(resolve_crs(None, Some(4326)).unwrap(), Crs::Wgs84);
    assert_eq!(resolve_crs(None, Some(3785)).unwrap(), Crs::WebMercator);
}

#[test]
fn unusable_or_missing_crs_errors() {
    assert!(resolve_crs(None, Some(3395)).is_err());
    assert!(resolve_crs(None, None).is_err());
}
