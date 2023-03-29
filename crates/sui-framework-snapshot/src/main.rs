use std::fs;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use sui_framework::make_system_packages;
use sui_types::move_package::MovePackage;

fn main() {
    for package in make_system_packages() {
        write_package_to_file(&package.id().to_string(), &package);
    }
}

fn write_package_to_file(package_id: &str, package: &MovePackage) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bytecode_snapshot");
    fs::create_dir(&path)
        .or_else(|e| match e.kind() {
            std::io::ErrorKind::AlreadyExists => Ok(()),
            _ => Err(e),
        })
        .expect("Unable to create snapshot directory");
    let bytes = bcs::to_bytes(package).expect("Deserialization cannot fail");
    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true) // Truncate file to zero length if it exists
        .create(true)
        .open(path.join(package_id))
        .expect("Unable to open file"); // Open file to write to

    // Write the data to the file
    file.write_all(&bytes)
        .expect("Unable to write data to file");
}
