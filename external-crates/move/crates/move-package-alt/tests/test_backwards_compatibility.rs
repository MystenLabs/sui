use std::fs;
use std::path::Path;

use move_package_alt::{
    compatibility::legacy_manifest_parser::{parse_legacy_manifest_from_file},
    flavor::Vanilla,
};

#[tokio::test]
async fn test_legacy_parsing(){
    let folder = Path::new("tests/compatibility/data");
    
    let sub_folders = fs::read_dir(folder).unwrap();

    for sub_folder in sub_folders {
        let sub_folder = sub_folder.unwrap();


        let (manifest, legacy_package_info) = parse_legacy_manifest_from_file::<Vanilla>(&sub_folder.path()).unwrap();

        eprintln!("{:?}", manifest);
        eprintln!("{:?}", legacy_package_info);
        eprintln!("--------------------------------");
        // println!("{}", manifest);
    }
}
