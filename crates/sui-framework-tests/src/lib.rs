// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod test {
    use move_cli::base::test::UnitTestResult;
    use move_unit_test::UnitTestingConfig;
    use std::path::{Path, PathBuf};
    use sui_framework::build_move_package;
    use sui_framework_build::compiled_package::BuildConfig;
    use sui_move::unit_test::run_move_unit_tests;

    #[test]
    #[cfg_attr(msim, ignore)]
    fn run_framework_move_unit_tests() {
        for package in ["sui-framework", "sui-system"] {
            check_move_unit_tests(&{
                let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                buf.extend(["..", "sui-framework", "packages", package]);
                buf
            });
        }
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn run_examples_move_unit_tests() {
        for example in [
            "basics",
            "defi",
            "capy",
            "fungible_tokens",
            "games",
            "move_tutorial",
            "nfts",
            "objects_tutorial",
        ] {
            check_move_unit_tests(&{
                let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                buf.extend(["..", "..", "sui_programmability", "examples", example]);
                buf
            });
        }
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn run_book_examples_move_unit_tests() {
        check_move_unit_tests(&{
            let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            buf.extend(["..", "..", "doc", "book", "examples"]);
            buf
        });
    }

    fn check_move_unit_tests(path: &Path) {
        let mut config = BuildConfig::new_for_testing();
        // Make sure to verify tests
        config.config.dev_mode = true;
        config.config.test_mode = true;
        config.run_bytecode_verifier = true;
        config.print_diags_to_stderr = true;
        let move_config = config.config.clone();
        let testing_config = UnitTestingConfig::default_with_bound(Some(3_000_000));

        // build tests first to enable Sui-specific test code verification
        build_move_package(path, config)
            .unwrap_or_else(|e| panic!("Building tests at {}.\nWith error {e}", path.display()));

        assert_eq!(
            run_move_unit_tests(path, move_config, Some(testing_config), false).unwrap(),
            UnitTestResult::Success
        );
    }
}
