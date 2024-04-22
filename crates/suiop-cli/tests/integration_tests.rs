use anyhow::Result;
use std::process::Command;
use suioplib::cli::service::init;

#[cfg(test)]
#[test]
fn test_initialize_service() -> Result<()> {
    // create a temp dir to work in
    let temp_dir = tempfile::tempdir().expect("creating temp dir");
    let svc_dir = temp_dir.path().join("svc");
    std::fs::create_dir(&svc_dir)?;

    // Run the command to initialize a new service
    init::bootstrap_service(&init::ServiceLanguage::Rust, &svc_dir)?;
    // Check that the Cargo.toml file was created
    assert!(svc_dir.join("Cargo.toml").exists());

    // Check that we can run `cargo build` in the new directory
    let output = Command::new("cargo")
        .arg("build")
        .current_dir(svc_dir)
        .output()?;

    println!("cargo build output: {:?}", output);
    assert!(output.status.success());
    Ok(())
}
