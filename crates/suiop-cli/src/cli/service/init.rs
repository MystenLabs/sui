// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Context;
use anyhow::Result;
use clap::Parser;
use clap::ValueEnum;
use include_dir::{include_dir, Dir};
use std::fs;
use std::fs::create_dir_all;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use tracing::debug;
use tracing::info;

// include the boilerplate code in this binary
static PROJECT_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/boilerplate");

#[derive(ValueEnum, Parser, Debug, Clone)]
pub enum ServiceLanguage {
    Rust,
    Typescript,
}

pub fn bootstrap_service(lang: &ServiceLanguage, path: &Path) -> Result<()> {
    match lang {
        ServiceLanguage::Rust => create_rust_service(path),
        ServiceLanguage::Typescript => todo!(),
    }
}

/// Add the new service to the sui-services dockerfile in the sui repository
fn add_to_sui_dockerfile(path: &Path) -> Result<()> {
    let path = path.canonicalize().context("canonicalizing service path")?;
    let crates_dir = path.parent().unwrap();
    if !crates_dir.ends_with("sui/crates") {
        panic!("directory wasn't in the sui repo");
    }
    let sui_services_dockerfile_path = &crates_dir.join("../docker/sui-services/Dockerfile");
    // read the dockerfile
    let dockerfile = fs::read_to_string(sui_services_dockerfile_path)
        .context("reading sui-services dockerfile")?;

    // find the line with the build cmd
    let build_line = dockerfile
        .lines()
        .enumerate()
        .find(|(_, line)| line.contains("RUN cargo build --release \\"))
        .expect("couldn't find build line in sui-services dockerfile")
        .0;
    // update with the new service
    let mut final_dockerfile = dockerfile.lines().collect::<Vec<_>>();
    let bin_str = format!(
        "    --bin {} \\",
        path.file_name()
            .expect("getting the project name from the given path")
            .to_str()
            .unwrap()
    );
    final_dockerfile.insert(build_line + 1, &bin_str);
    // write the file back
    fs::write(sui_services_dockerfile_path, final_dockerfile.join("\n"))
        .context("writing sui-services dockerfile after modifying it")?;

    Ok(())
}

fn add_member_to_workspace(path: &Path) -> Result<()> {
    // test
    let path = path.canonicalize().context("canonicalizing service path")?;
    let crates_dir = path.parent().context("getting path parent").unwrap();
    if !crates_dir.ends_with("sui/crates") {
        panic!("directory wasn't in the sui repo");
    }
    let workspace_toml_path = &crates_dir.join("../Cargo.toml");
    // read the workspace toml
    let toml_content =
        fs::read_to_string(workspace_toml_path).context("reading workspace cargo file")?;
    let mut toml = toml_content
        .parse::<toml_edit::Document>()
        .context("parsing workspace cargo file")?;
    toml["workspace"]["members"]
        .as_array_mut()
        .unwrap()
        .push_formatted(toml_edit::Value::String(toml_edit::Formatted::new(
            Path::new("crates/")
                .join(
                    path.file_name()
                        .expect("getting the project name from the given path"),
                )
                .to_str()
                .expect("converting the path to a str")
                .to_string(),
        )));
    fs::write(workspace_toml_path, toml.to_string())
        .context("failed to write workspace Cargo.toml back after update")?;
    Ok(())
}

fn create_rust_service(path: &Path) -> Result<()> {
    info!("creating rust service in {}", path.to_string_lossy());
    // create the dir to ensure we can canonicalize any relative paths
    create_dir_all(path).context("creating rust service dirs")?;
    let is_sui_service = path
        // expand relative paths and symlinks
        .canonicalize()
        .context("canonicalizing service path")?
        .to_string_lossy()
        .contains("sui/crates");
    debug!("sui service: {:?}", is_sui_service);
    let cargo_toml_path = if is_sui_service {
        "Cargo-sui.toml"
    } else {
        "Cargo-ext.toml"
    };
    let cargo_toml = PROJECT_DIR
        .get_file(cargo_toml_path)
        .context("getting cargo toml file from boilerplate")
        .unwrap();
    let main_rs = PROJECT_DIR
        .get_file("src/main.rs")
        .context("getting main.rs file from boilerplate")
        .unwrap();
    let main_body = main_rs.contents();
    let cargo_body =
        std::str::from_utf8(cargo_toml.contents()).context("decoding cargo toml body")?;
    let mut toml_content = cargo_body
        .parse::<toml_edit::Document>()
        .context("parsing cargo toml file")?;
    toml_content["package"]["name"] = toml_edit::value(
        path.file_name()
            .context("peeling tail off of path")
            .unwrap()
            .to_str()
            .context("decoding dir to str")
            .unwrap(),
    );
    create_dir_all(path.join("src")).context("creating src dir")?;
    let mut main_file = File::create(path.join("src/main.rs")).context("creating main.rs file")?;
    main_file
        .write_all(main_body)
        .context("writing main.rs file")?;
    let mut cargo_file =
        File::create(path.join("Cargo.toml")).context("creating cargo toml file")?;
    cargo_file
        .write_all(toml_content.to_string().as_bytes())
        .context("writing cargo toml file")?;

    // add the project as a member of the cargo workspace
    if is_sui_service {
        add_member_to_workspace(path).context("adding crate to sui workspace")?;
    }
    // now that the source directory works, let's update/add a dockerfile
    if is_sui_service {
        // update sui-services dockerfile
        add_to_sui_dockerfile(path).context("adding crate to sui services dockerfile")?;
    } else {
        // TODO: create a new dockerfile where the user designates
    }

    Ok(())
}
