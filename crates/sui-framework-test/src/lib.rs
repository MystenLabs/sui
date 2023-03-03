// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[cfg(test)]
mod test {
    use move_cli::base::test::UnitTestResult;
    use move_package::BuildConfig as MoveBuildConfig;
    use std::path::{Path, PathBuf};
    use sui_framework::build_move_package;
    use sui_framework_build::compiled_package::BuildConfig;
    use sui_move::unit_test::run_move_unit_tests;

    #[test]
    #[cfg_attr(msim, ignore)]
    fn run_framework_move_unit_tests() {
        let path = {
            let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            buf.extend(["..", "sui-framework"]);
            buf
        };

        BuildConfig::new_for_testing().build(path.clone()).unwrap();
        check_move_unit_tests(&path);
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
            let path = {
                let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
                buf.extend(["..", "..", "sui_programmability", "examples", example]);
                buf
            };
            BuildConfig::new_for_testing().build(path.clone()).unwrap();
            check_move_unit_tests(&path);
        }
    }

    #[test]
    #[cfg_attr(msim, ignore)]
    fn run_book_examples_move_unit_tests() {
        let path = {
            let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
            buf.extend(["..", "..", "doc", "book", "examples"]);
            buf
        };

        BuildConfig::new_for_testing().build(path.clone()).unwrap();
        check_move_unit_tests(&path);
    }

    fn check_move_unit_tests(path: &Path) {
        // build tests first to enable Sui-specific test code verification
        matches!(
            build_move_package(
                path,
                BuildConfig {
                    config: MoveBuildConfig {
                        test_mode: true, // make sure to verify tests
                        ..MoveBuildConfig::default()
                    },
                    run_bytecode_verifier: true,
                    print_diags_to_stderr: true,
                },
            ),
            Ok(_)
        );
        assert_eq!(
            run_move_unit_tests(path, MoveBuildConfig::default(), None, false).unwrap(),
            UnitTestResult::Success
        );
    }
}
