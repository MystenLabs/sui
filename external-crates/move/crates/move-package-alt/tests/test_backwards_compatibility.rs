use std::fs;
use std::path::Path;

use move_package_alt::{flavor::Vanilla, package::Package};

#[tokio::test]
async fn test_legacy_parsing() {
    let folder = Path::new("tests/compatibility/data");

    let sub_folders = fs::read_dir(folder).unwrap();

    for sub_folder in sub_folders {
        let sub_folder = sub_folder.unwrap();

        let package = Package::<Vanilla>::load_root(sub_folder.path())
            .await
            .unwrap();

        eprintln!("{:?}", package);
        eprintln!("--------------------------------");
    }
}
