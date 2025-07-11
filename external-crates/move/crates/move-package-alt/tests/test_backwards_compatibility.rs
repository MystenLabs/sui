use std::fs;
use std::path::Path;

use move_package_alt::{
    flavor::Vanilla,
    package::{Package, paths::PackagePath, root_package::RootPackage},
};

/// TODO(manos): Finish these tests here -- right now they only check that there's no
/// failure, but bring no value checks etc.
///
/// Will need to build the proper testing on top of the test framework that's being worked on
/// instead.
#[tokio::test]
async fn test_legacy_parsing() {
    let folder = Path::new("tests/compatibility");

    let sub_folders = fs::read_dir(folder).unwrap();

    for sub_folder in sub_folders {
        let sub_folder = sub_folder.unwrap();

        let package = Package::<Vanilla>::load_root(sub_folder.path())
            .await
            .unwrap();

        // todo: probably snapshot test the parsing --
        // though that'll be done more easily on the "migrate"
        // commands.

        eprintln!("{:?}", package);
        eprintln!("--------------------------------");
    }
}

#[tokio::test]
async fn test_modern_with_legacy() {
    let folder = Path::new("tests/compatibility/compatibility_modern_with_legacy");

    let package_path = PackagePath::new(folder.to_path_buf()).unwrap();
    let graph = RootPackage::<Vanilla>::load(&package_path, None)
        .await
        .unwrap();

    eprintln!("{:?}", graph);
}
