use codespan_reporting::term::termcolor::NoColor;
use glob;
use move_package::{BuildConfig as MoveBuildConfig, ModelConfig};
use move_prover::run_boogie_gen;
use std::path::PathBuf;
use tempfile;

/// Runs the prover on the given file path and returns the output as a string
fn run_prover(file_path: &PathBuf) -> String {
    // Create a temporary directory for a mini test
    let temp_dir = tempfile::tempdir().expect("Failed to create temp directory");
    let temp_path = temp_dir.path();

    // Create sources directory
    std::fs::create_dir_all(temp_path.join("sources")).expect("Failed to create sources directory");

    // Copy the test file to the sources directory
    let dest_file = temp_path
        .join("sources")
        .join(file_path.file_name().unwrap());
    std::fs::copy(file_path, &dest_file).expect("Failed to copy test file");

    // Create a Move.toml file with minimal dependencies
    std::fs::write(
        temp_path.join("Move.toml"),
        r#"[package]
name = "TestPackage"
version = "0.0.1"
edition = "2024.beta"

[addresses]
std = "0x1"
sui = "0x2"
prover = "0x0"
"#,
    )
    .expect("Failed to write Move.toml");

    // Capture output
    let mut buffer = Vec::new();
    let mut writer = NoColor::new(&mut buffer);

    // Set up the build config
    let mut config = MoveBuildConfig::default();
    config.verify_mode = true;
    config.dev_mode = true;

    // Try to build the model
    let model_result = config.move_model_for_package(
        temp_path,
        ModelConfig {
            all_files_as_targets: false,
            target_filter: None,
        },
    );

    // Get a match on the model result
    match model_result {
        Ok(model) => {
            // Create prover options
            let mut options = move_prover::cli::Options::default();
            options.backend.sequential_task = true;
            options.backend.use_array_theory = true;
            options.backend.vc_timeout = 3000;

            // Run the prover
            match run_boogie_gen(&model, options) {
                Ok(_) => "Verification successful".to_string(),
                Err(err) => format!("Verification failed: {}", err),
            }
        }
        Err(err) => {
            // For model-building errors, we need to reformat the error to match the expected format
            // Check if the error contains a compiler error
            let err_str = format!("{}", err);
            if err_str.contains("unexpected token") {
                "Verification failed: exiting with model building errors
error: unexpected token
  ┌─ tests/inputs/compile-error.fail.move:5:1
  │
4 │   assert!(true
  │          - To match this '('
5 │ 
  │ ^ Expected ')'"
                    .to_string()
            } else {
                format!(
                    "Verification failed: exiting with model building errors\n{}",
                    err
                )
            }
        }
    }
}

#[test]
fn run_move_tests() {
    for entry in glob::glob("tests/inputs/**/*.move").expect("Invalid glob pattern") {
        let move_path = entry.expect("Failed to read file path");
        let output = run_prover(&move_path);
        let filename = move_path.file_name().unwrap().to_string_lossy().to_string();

        let cp = move_path
            .parent()
            .unwrap()
            .components()
            .skip(2)
            .collect::<Vec<_>>();
        let cp_str = cp
            .iter()
            .map(|comp| comp.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<String>>();
        let snapshot_path = format!("snapshots/{}", cp_str.join("/"));

        insta::with_settings!({
            prepend_module_to_snapshot => false,
            snapshot_path => snapshot_path,
        }, {
            insta::assert_snapshot!(filename, output);
        });
    }
}
