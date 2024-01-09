// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{Context, Result};
use std::io::Write;
use toml_edit::Document;

#[test]
fn test_adjust_pyproject() -> Result<()> {
    // using common code.
    assert_eq!(4, 4);
    let tempdir_path = tempfile::tempdir()
        .context("creating temp dir")?
        .into_path();
    let pyproject_path = tempdir_path.join("pyproject.toml");
    let mut pyproject_toml = std::fs::File::create(&pyproject_path).context("creating file")?;
    pyproject_toml
        .write_all(
            br#"
[tool.poetry]
name = "dummy-package"
version = "0.1.0"
description = "dummy"
authors = ["mysten labs <info@mystenlabs.com>"]
readme = "README.md"
packages = [{ include = "dummy_package" }]

[tool.poetry.dependencies]
python = "^3.11"
sui-pulumi-common = {path = "../../common"}
pulumi = "^3.75.0"
pulumi-kubernetes = "^3.30.2"


[build-system]
requires = ["poetry-core"]
build-backend = "poetry.core.masonry.api"
"#,
        )
        .context("writing to file")?;
    suioplib::cli::pulumi::adjust_pyproject(&tempdir_path).context("running adjust_pyproject")?;
    let pyproject_contents =
        std::fs::read(&pyproject_path).context("couldn't read config contents")?;
    let pyproject_toml_contents = std::str::from_utf8(&pyproject_contents)
        .context("failed to parse pyproject contents")?
        .parse::<Document>()
        .expect("invalid toml");
    assert_eq!(
        pyproject_toml_contents["tool"]["poetry"]["packages"][0]["include"]
            .as_str()
            .expect("include couldn't be coerced into a str"),
        "**/*.py"
    );
    assert!(!pyproject_toml_contents["tool"]["poetry"]
        .as_table()
        .expect("can't coerce tool.poetry to table")
        .contains_key("readme"));
    Ok(())
}
