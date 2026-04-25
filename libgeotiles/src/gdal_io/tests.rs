use super::open_dataset;

#[test]
fn open_nonexistent_returns_error() {
    let result = open_dataset(std::path::Path::new("/definitely/does/not/exist/input.tif"));
    assert!(result.is_err(), "expected error for nonexistent path");
}
