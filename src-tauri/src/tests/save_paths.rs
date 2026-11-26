use std::path::PathBuf;

use crate::logic::ensure_extension;

#[test]
fn save_path_gets_its_extension_only_when_missing() {
    assert_eq!(
        ensure_extension(PathBuf::from("/tmp/activity"), "fit"),
        PathBuf::from("/tmp/activity.fit")
    );
    assert_eq!(
        ensure_extension(PathBuf::from("/tmp/activity.FIT"), "fit"),
        PathBuf::from("/tmp/activity.FIT")
    );
    assert_eq!(
        ensure_extension(PathBuf::from("/tmp/v1.2"), "fit"),
        PathBuf::from("/tmp/v1.2.fit")
    );
}
