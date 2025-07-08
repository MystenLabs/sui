use std::fs;
use std::path::Path;

use move_package_alt::{
    flavor::Vanilla,
    graph::PackageGraph,
    package::{Package, paths::PackagePath},
};

// TODO: Finish the testing here, besides the basic ones I've added.
#[tokio::test]
async fn test_legacy_parsing() {
    let folder = Path::new("tests/compatibility/data");

    let sub_folders = fs::read_dir(folder).unwrap();

    for sub_folder in sub_folders {
        let sub_folder = sub_folder.unwrap();

        let package = Package::<Vanilla>::load_root(sub_folder.path())
            .await
            .unwrap();

        // let addr

        eprintln!("{:?}", package);
        eprintln!("--------------------------------");
    }
}

#[tokio::test]
async fn test_modern_with_legacy() {
    let folder = Path::new("tests/compatibility/data/modern_with_legacy");

    let package_path = PackagePath::new(folder.to_path_buf()).unwrap();
    let graph = PackageGraph::<Vanilla>::load(&package_path).await.unwrap();

    eprintln!("{:?}", graph);
}
