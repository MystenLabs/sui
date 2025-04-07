use glob;
use move_package::{BuildConfig as MoveBuildConfig, ModelConfig};
use move_prover::run_boogie_gen;
use std::path::{Path, PathBuf};

/// Runs the prover on the given file path and returns the output as a string
fn run_prover(file_path: &PathBuf) -> String {
    // the file_dir path is `tests`, make it as a Path
    let file_dir = Path::new("tests");
    let sources_dir = file_dir.join("sources");

    // Keep track of files we renamed
    let mut renamed_files = Vec::new();

    // rename all .move files in the directory to .move.bak
    // skip the file_path
    for entry in std::fs::read_dir(sources_dir).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_file()
            && path.extension().map_or(false, |ext| ext == "move")
            && path != *file_path
        {
            std::fs::rename(&path, path.with_extension("move.bak")).unwrap();
            renamed_files.push(path.clone());
        }
    }

    // Setup cleanup that will execute even in case of panic or early return
    let result = std::panic::catch_unwind(|| {
        // Capture output
        let mut buffer: Vec<u8> = Vec::new();

        // Set up the build config
        let mut config = MoveBuildConfig::default();
        config.verify_mode = true;
        config.dev_mode = true;

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

                // Run the prover
                match run_boogie_gen(&model, options) {
                    Ok(_) => "Verification successful".to_string(),
                    Err(err) => format!("Verification failed: {}", err),
                }
            }
            Err(err) => {
                // For model-building errors, we need to reformat the error to match the expected format
                format!(
                    "Verification failed: exiting with model building errors\n{}",
                    err
                )
            }
        };

        result
    });

    // ALWAYS perform cleanup regardless of success or failure
    for path in renamed_files {
        let backup_path = path.with_extension("move.bak");
        if backup_path.exists() {
            let _ = std::fs::rename(&backup_path, &path);
        }
    }

    // Now handle the result of our operation
    match result {
        Ok(output) => output,
        Err(_) => "Verification failed: panic during verification".to_string(),
    }
}

#[test]
fn run_move_tests() {
    for entry in glob::glob("tests/sources/**/*.move").expect("Invalid glob pattern") {
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
