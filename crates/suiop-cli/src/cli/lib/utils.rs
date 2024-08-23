// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Result};
use regex::Regex;

/// Validates the format of a project name.
///
/// The name must start with a lowercase letter and can only contain
/// lowercase alphanumeric characters and dashes, up to 30 characters in total.
pub fn validate_project_name(project_name: &str) -> Result<()> {
    let project_name_validation_regex = Regex::new(r"^[a-z][a-z0-9\-]{0,29}$").unwrap();
    if !project_name_validation_regex.is_match(project_name) {
        Err(anyhow!("project_name should start with a letter and only contain alphanumeric chars or dashes."))
    } else {
        Ok(())
    }
}
