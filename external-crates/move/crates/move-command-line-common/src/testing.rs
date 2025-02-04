// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::env::read_bool_env_var;

/// Extension for raw output files
pub const OUT_EXT: &str = "out";
/// Extension for expected output files
pub const EXP_EXT: &str = "exp";

/// If any of these env vars is set, the test harness should overwrite
/// the existing .exp files with the output instead of checking
/// them against the output.
pub const UPDATE_BASELINE: &str = "UPDATE_BASELINE";
pub const UPBL: &str = "UPBL";
pub const UB: &str = "UB";

pub const PRETTY: &str = "PRETTY";
pub const FILTER: &str = "FILTER";

pub fn read_env_update_baseline() -> bool {
    read_bool_env_var(UPDATE_BASELINE) || read_bool_env_var(UPBL) || read_bool_env_var(UB)
}

pub fn add_update_baseline_fix(s: impl AsRef<str>) -> String {
    format!(
        "{}\n\
        Run with `env {}=1` (or `env {}=1`) to save the current output as \
        the new expected output",
        s.as_ref(),
        UB,
        UPDATE_BASELINE
    )
}

pub fn format_diff(expected: impl AsRef<str>, actual: impl AsRef<str>) -> String {
    use difference::*;

    let changeset = Changeset::new(expected.as_ref(), actual.as_ref(), "\n");

    let mut ret = String::new();

    for seq in changeset.diffs {
        match &seq {
            Difference::Same(x) => {
                ret.push_str(x);
                ret.push('\n');
            }
            Difference::Add(x) => {
                ret.push_str("\x1B[92m");
                ret.push_str(x);
                ret.push_str("\x1B[0m");
                ret.push('\n');
            }
            Difference::Rem(x) => {
                ret.push_str("\x1B[91m");
                ret.push_str(x);
                ret.push_str("\x1B[0m");
                ret.push('\n');
            }
        }
    }
    ret
}

/// See `insta_assert!` for documentation.
pub struct InstaOptions<Info: serde::Serialize, Suffix: Into<String>> {
    pub info: Option<Info>,
    pub suffix: Option<Suffix>,
}

impl<Info: serde::Serialize, Suffix: Into<String>> InstaOptions<Info, Suffix> {
    /// See `insta_assert!` for documentation.
    pub fn new() -> Self {
        Self {
            info: None,
            suffix: None,
        }
    }
}

impl InstaOptions<(), String> {
    /// See `insta_assert!` for documentation.
    pub fn none() -> Self {
        Self {
            info: None,
            suffix: None,
        }
    }
}

// This `pub use` allows for `insta` to be used easily in the macro
pub use insta;

#[macro_export]
/// A wrapper around `insta::assert_snapshort` to promote uniformity in the Move codebase, intended
/// to be used with datatest-stable and as a replacement for the hand-rolled baseline tests.
/// The snapshot file will be saved in the same directory as the input file with the name specified.
/// In essence, it will be saved at the path `{input_path}/{name}.snap` (and
/// `{input_path}/{name}@{suffix}.snap` if `suffix` is specified).
///
/// For ease of use and reviewing, `insta_assert` should be used at most once per test. When it
/// fails, it will stop the test. So if there are multiple snapshots in a given test, it would
/// require multiple test runs to review all the failures.
/// If you do need multiple snapshots in a test, you may want to disable assertions for your test
/// run by setting `INSTA_FORCE_PASS=1` see
/// https://insta.rs/docs/advanced/#disabling-assertion-failure for more information.
///
/// # Arguments
/// The macro has three required arguments:
///
/// - `name`: The name of the test. This will be used to name the snapshot file. For datatest this
///           should likely be the file name.
/// - `input_path`: The path to the input file. This is used to determine the snapshot path.
/// - `contents`: The contents to snapshot.
///
///
/// The macro also accepts an optional arguments to that are used with `InstaOptions` to customize
/// the snapshot. If needed the `InstaOptions` struct can be used directly by specifying the
/// `options` argument. Options include:
///
///  - `info`: Additional information to include in the header of the snapshot file. This can be
///           useful for debugging tests. The value can be any type that implements
///           `serde::Serialize`.
/// - `suffix`: A suffix to append to the snapshot file name. This changes the snapshot path to
///            `{input_path}/{name}@{suffix}.snap`.
///
/// # Updating snapshots
///
/// After running the test, the `.snap` files can be updated in two ways:
///
/// 1. By using `cargo insta review`, which will open an interactive UI to review the changes.
/// 2. Running the tests with the environment variable `INSTA_UPDATE=alawys`
///
/// See https://docs.rs/insta/latest/insta/#updating-snapshots for more information.
macro_rules! insta_assert {
    {
        name: $name:expr,
        input_path: $input:expr,
        contents: $contents:expr,
        options: $options:expr
        $(,)?
    } => {{
        let name: String = $name.into();
        let i: &std::path::Path = $input.as_ref();
        let c = $contents;
        let $crate::testing::InstaOptions { info, suffix } = $options;
        let mut settings = $crate::testing::insta::Settings::clone_current();
        let output = settings.set_snapshot_path( i.canonicalize().unwrap().parent().unwrap());
        if let Some(info) = info {
            settings.set_info(&info);
        }
        if let Some(suffix) = suffix {
            settings.set_snapshot_suffix(suffix);
        }
        settings.set_prepend_module_to_snapshot(false);
        settings.set_omit_expression(true);
        settings.bind(|| {
            $crate::testing::insta::assert_snapshot!(name, c);
        });
    }};
    {
        name: $name:expr,
        input_path: $input:expr,
        contents: $contents:expr
        $(,)?
    } => {{
        insta_assert! {
            name: $name,
            input_path: $input,
            contents: $contents,
            options: $crate::testing::InstaOptions::none(),
        }
    }};
    {
        name: $name:expr,
        input_path: $input:expr,
        contents: $contents:expr,
        $($k:ident: $v:expr),+$(,)?
    } => {{
        let mut opts = $crate::testing::InstaOptions::new();
        $(
            opts.$k = Some($v);
        )+
        insta_assert! {
            name: $name,
            input_path: $input,
            contents: $contents,
            options: opts
        }
    }};
}
pub use insta_assert;
