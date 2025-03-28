use codespan_reporting::term::termcolor::NoColor;
use glob;
use move_prover::{cli::Options, run_move_prover};
use std::path::PathBuf;

/// Runs the prover on the given file path and returns the output as a string
fn run_prover(file_path: &PathBuf) -> String {
    // Create prover options
    let mut options = Options::default();
    options.move_sources = vec![file_path.to_string_lossy().to_string()];
    options.prover.stable_test_output = true; // For consistent snapshot testing
    options.prover.report_severity = codespan_reporting::diagnostic::Severity::Error; // Only show errors, suppress warnings

    // Always add standard Sui dependencies for all tests
    let sui_packages_base = "../../../../crates/sui-framework/packages";
    options
        .move_deps
        .push(format!("{}/move-stdlib", sui_packages_base));
    options
        .move_deps
        .push(format!("{}/prover", sui_packages_base));

    // Add basic named address values for all tests
    options.move_named_address_values = vec!["std=0x1".to_string(), "prover=0x0".to_string()];

    // Capture output using a Vec buffer with NoColor writer
    let mut buffer = Vec::new();
    let mut writer = NoColor::new(&mut buffer);

    // Run the prover and capture output
    let result = match run_move_prover(&mut writer, options) {
        Ok(_) => "Verification successful".to_string(),
        Err(err) => format!("Verification failed: {}", err),
    };

    // Combine the prover result with any captured output
    let captured_output = String::from_utf8_lossy(&buffer).to_string();
    if captured_output.is_empty() {
        result
    } else {
        format!("{}\n{}", result, captured_output)
    }
}

#[test]
fn run_move_tests() {
    for entry in glob::glob("tests/inputs/*.move").expect("Invalid glob pattern") {
        let move_path = entry.expect("Failed to read file path");

        let output = run_prover(&move_path);

        let filename = move_path.file_name().unwrap().to_string_lossy().to_string();

        insta::with_settings!({
            prepend_module_to_snapshot => false
        }, {
            insta::assert_snapshot!(filename, output);
        });
    }
}
