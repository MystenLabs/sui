// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use once_cell::sync::Lazy;

/// An environment variable which can be set to cause the move compiler to generate
/// file formats at a given version. Only version v5 and greater are supported.
const BYTECODE_VERSION_ENV_VAR: &str = "MOVE_BYTECODE_VERSION";

/// Get the bytecode version from the environment variable.
// TODO: This should be configurable via toml and command line flags. See also #129.
pub fn get_bytecode_version_from_env() -> Option<u32> {
    std::env::var(BYTECODE_VERSION_ENV_VAR)
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
}

pub fn read_env_var(v: &str) -> String {
    std::env::var(v).unwrap_or_else(|_| String::new())
}

pub fn read_bool_env_var(v: &str) -> bool {
    let val = read_env_var(v).to_lowercase();
    val.parse::<bool>() == Ok(true) || val.parse::<usize>() == Ok(1)
}

pub static MOVE_HOME: Lazy<String> = Lazy::new(|| {
    std::env::var("MOVE_HOME").unwrap_or_else(|_| {
        format!(
            "{}/.move",
            dirs_next::home_dir()
                .expect("user's home directory not found")
                .to_str()
                .unwrap()
        )
    })
});
