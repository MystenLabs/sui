use codespan_reporting::term::termcolor::Buffer;
use glob;
use move_compiler::editions::Flavor;
use move_package::BuildConfig as MoveBuildConfig;
use move_prover::run_move_prover_with_model;
use regex::Regex;
use std::path::{Path, PathBuf};

/// Runs the prover on the given file path and returns the output as a string
fn run_prover(file_path: &PathBuf) -> String {
    // the file_dir path is `tests`, make it as a Path
    let file_dir = Path::new("tests");
    let sources_dir = file_dir.join("sources");
    // create the sources_dir if it doesn't exist
    if !sources_dir.clone().exists() {
        std::fs::create_dir_all(sources_dir.clone()).unwrap();
    }

    // Extract the relative path from tests/inputs/
    let relative_path = file_path
        .strip_prefix(Path::new("tests/inputs"))
        .unwrap_or_else(|_| Path::new(file_path.file_name().unwrap()));

    // Join it to the sources directory
    let new_file_path = sources_dir.join(relative_path);

    // Create parent directories if needed
    if let Some(parent_dir) = new_file_path.parent() {
        std::fs::create_dir_all(parent_dir).unwrap();
    }

    std::fs::rename(file_path, &new_file_path).unwrap();

    let new_file_path_clone = new_file_path.clone();

    // Setup cleanup that will execute even in case of panic or early return
    let result = std::panic::catch_unwind(|| {
        // Set up the build config
        let mut config = MoveBuildConfig::default();
        config.default_flavor = Some(Flavor::Sui);
        config.verify_mode = true;
        config.silence_warnings = false; // Disable warning suppression

        // Try to build the model
        let result = match config.move_model_for_package_legacy(
            file_dir,
        ) {
            Ok(model) => {
                // Create prover options
                let mut options = move_prover::cli::Options::default();
                options.backend.sequential_task = true;
                options.backend.use_array_theory = true;
                options.backend.vc_timeout = 3000;

                // Use a buffer to capture output instead of stderr
                let mut error_buffer = Buffer::no_color();

                // Run the prover with the buffer to capture all output
                match run_move_prover_with_model(&model, &mut error_buffer, options, None) {
                    Ok(_) => {
                        let error_output =
                            String::from_utf8_lossy(&error_buffer.into_inner()).to_string();
                        format!("Verification successful\n{}", error_output)
                    }
                    Err(err) => {
                        // Get the captured error output as string
                        let error_output =
                            String::from_utf8_lossy(&error_buffer.into_inner()).to_string();
                        format!("{}\n{}", err, error_output)
                    }
                }
            }
            Err(err) => {
                // For model-building errors, we need to reformat the error to match the expected format
                format!("We hit an error: \n{}", err)
            }
        };

        post_process_output(result)
    });

    // rename the file_path to the original name
    std::fs::rename(new_file_path_clone, file_path).unwrap();

    // Now handle the result of our operation
    match result {
        Ok(output) => output,
        Err(_) => "Verification failed: panic during verification".to_string(),
    }
}

fn post_process_output(output: String) -> String {
    // replace numbers such as 52571u64 with ELIDEDu64 to avoid snapshot diffs
    let output = output.replace("tests/sources/", "tests/inputs/");

    // Use regex to replace numbers with more than one digit followed by u64 with ELIDEDu64
    let re = Regex::new(r"\d{2,}u64").unwrap();
    re.replace_all(&output, "ELIDEDu64").to_string()
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
