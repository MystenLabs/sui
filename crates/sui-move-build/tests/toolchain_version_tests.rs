use std::path::PathBuf;

use sui_move_build::BuildConfig;

#[test]
fn resolve_toolchain_version() {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend([
        "tests",
        "toolchain_version",
        "fixture",
        "a_resolve_move_lock",
    ]);
    let pkg = BuildConfig::new_for_testing().build(path).unwrap();
}
