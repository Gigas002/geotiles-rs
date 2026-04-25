use super::*;

#[test]
fn dataset_bounds_north_up() {
    let gt = [-180.0, 0.5, 0.0, 90.0, 0.0, -0.5];
    let bounds = dataset_bounds(&gt, 720, 360);
    assert!((bounds.min_x - (-180.0)).abs() < 1e-9);
    assert!((bounds.max_x - 180.0).abs() < 1e-9);
    assert!((bounds.max_y - 90.0).abs() < 1e-9);
    assert!((bounds.min_y - (-90.0)).abs() < 1e-9);
}

#[test]
fn dataset_bounds_partial() {
    let gt = [10.0, 1.0, 0.0, 50.0, 0.0, -1.0];
    let bounds = dataset_bounds(&gt, 5, 5);
    assert!((bounds.min_x - 10.0).abs() < 1e-9);
    assert!((bounds.max_x - 15.0).abs() < 1e-9);
    assert!((bounds.max_y - 50.0).abs() < 1e-9);
    assert!((bounds.min_y - 45.0).abs() < 1e-9);
}
