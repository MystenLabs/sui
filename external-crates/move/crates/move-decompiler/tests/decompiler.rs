mod utils;

#[cfg(test)]
mod test {

    use std::path::Path;
    use std::{env, fs};

    use super::utils;
    use move_compiler::Flags;
    use revela::decompiler::{Decompiler, OptimizerSettings};

    pub fn decompile_compile_decompile_match_single_file(
        path: &Path,
    ) -> datatest_stable::Result<()> {
        let module_name = path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .replace(".move", "");
        let source = fs::read_to_string(path).expect("Unable to read file");

        let corresponding_output_file = path.parent().unwrap().join(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .replace("-test.move", "-decompiled.move"),
        );

        let corresponding_second_output_file = path.parent().unwrap().join(
            path.file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .replace("-test.move", "-second-decompiled.move"),
        );

        let expected_result = fs::read_to_string(&corresponding_output_file);
        let mut src_modules = vec![];
        let mut output = String::new();

        let ref_output_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("refs");

        let env_revela_test_output_only = std::env::var("REVELA_TEST_OUTPUT_ONLY").ok().is_some();

        utils::tmp_project(vec![("tmp.move", source.as_str())], |tmp_files| {
            src_modules = utils::run_compiler(tmp_files, Flags::empty(), false);

            {
                let binaries = utils::into_binary_indexed_view(&src_modules);
                let mut decompiler = Decompiler::new(binaries, Default::default());
                let default_output = decompiler.decompile().expect("Unable to decompile");

                let ref_output_path =
                    ref_output_dir.join(format!("sources-{}-decompiled.move", module_name));
                std::fs::write(&ref_output_path, default_output).unwrap();
            }
            {
                let binaries = utils::into_binary_indexed_view(&src_modules);
                let mut decompiler = Decompiler::new(
                    binaries,
                    OptimizerSettings {
                        disable_optimize_variables_declaration: true,
                    },
                );
                output = decompiler.decompile().expect("Unable to decompile");
            }
        });

        if env_revela_test_output_only {
            println!("{}", output);
        } else if env::var("FORCE_UPDATE_EXPECTED_OUTPUT").is_ok() {
            fs::write(&corresponding_output_file, &output).unwrap();
        } else if let Ok(expected_result) = expected_result {
            utils::assert_same_source(&output, &expected_result);
        } else if env::var("UPDATE_EXPECTED_OUTPUT").is_ok() {
            fs::write(&corresponding_output_file, &output).unwrap();
        } else {
            panic!("Unable to read expected output file");
        }

        utils::tmp_project(vec![("tmp.move", output.as_str())], |tmp_files| {
            let modules = utils::run_compiler(tmp_files, Flags::empty(), false);

            let binaries = utils::into_binary_indexed_view(&modules);

            let mut decompiler = Decompiler::new(
                binaries,
                OptimizerSettings {
                    disable_optimize_variables_declaration: true,
                },
            );

            let output2 = decompiler.decompile().expect("Unable to decompile");

            if module_name.contains("-nodiff-") {
                fs::write(&corresponding_second_output_file, output2).unwrap();
            } else {
                utils::assert_same_source(&output, &output2);
            }
        });

        Ok(())
    }
}

datatest_stable::harness!(
    test::decompile_compile_decompile_match_single_file,
    "tests/sources",
    r"-test\.move$"
);
