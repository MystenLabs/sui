use codespan_reporting::term::termcolor::Buffer;
use codespan_reporting::term::termcolor::{ColorChoice, StandardStream, WriteColor};
use glob;
use move_compiler::editions::Flavor;
use move_model::model::GlobalEnv;
use move_package::{BuildConfig as MoveBuildConfig, ModelConfig};
use move_prover::run_boogie_gen;
use move_prover::run_move_prover_with_model;
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

    // move the file_path to the sources_dir
    let new_file_path = sources_dir.join(file_path.file_name().unwrap());
    std::fs::rename(file_path, &new_file_path).unwrap();

    let new_file_path_clone = new_file_path.clone();

    // Setup cleanup that will execute even in case of panic or early return
    let result = std::panic::catch_unwind(|| {
        // Set up the build config
        let mut config = MoveBuildConfig::default();
        config.default_flavor = Some(Flavor::Sui);
        config.verify_mode = true;

        // Try to build the model
        let result = match config.move_model_for_package(
            file_dir,
            ModelConfig {
                all_files_as_targets: false,
                target_filter: None,
            },
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
                    Ok(_) => "Verification successful".to_string(),
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

        result
    });

    // rename the file_path to the original name
    std::fs::rename(new_file_path_clone, file_path).unwrap();

    // Now handle the result of our operation
    match result {
        Ok(output) => output,
        Err(_) => "Verification failed: panic during verification".to_string(),
    }
}

#[test]
fn run_move_tests() {
    for entry in glob::glob("tests/inputs/**/*ints.fail.move").expect("Invalid glob pattern") {
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
