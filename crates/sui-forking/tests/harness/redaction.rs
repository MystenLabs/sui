// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::Path;

pub fn redact_snapshot_output(
    mut output: String,
    sandbox_dir: &Path,
    additional_replacements: &[(&str, &str)],
) -> String {
    if let Ok(canonical) = sandbox_dir.canonicalize() {
        output = output.replace(canonical.to_string_lossy().as_ref(), "<SANDBOX_DIR>");
    }
    output = output.replace(sandbox_dir.to_string_lossy().as_ref(), "<SANDBOX_DIR>");
    output = output.replace(r"\\", "/").replace('\\', "/");

    if let Ok(home) = std::env::var("HOME") {
        output = output.replace(&home, "<HOME>");
    }

    output = output.replace(
        "bash: warning: setlocale: LC_ALL: cannot change locale (C.UTF-8): No such file or directory\n",
        "",
    );

    for (source, target) in additional_replacements {
        if !source.is_empty() {
            output = output.replace(source, target);
        }
    }

    output
}
