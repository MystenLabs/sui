// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use clap::Parser;
use clap::ValueEnum;
use include_dir::{include_dir, Dir};
use std::fs::create_dir_all;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
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

fn create_rust_service(path: &Path) -> Result<()> {
    info!("creating rust service in {}", path.to_string_lossy());
    let cargo_toml_path = if path.to_string_lossy().contains("sui/crates") {
        "Cargo-sui.toml"
    } else {
        "Cargo.toml"
    };
    let cargo_toml = PROJECT_DIR.get_file(cargo_toml_path).unwrap();
    let main_rs = PROJECT_DIR.get_file("src/main.rs").unwrap();
    let main_body = main_rs.contents();
    let cargo_body = cargo_toml.contents();
    create_dir_all(path.join("src"))?;
    let mut main_file = File::create(path.join("src/main.rs"))?;
    main_file.write_all(main_body)?;
    let mut cargo_file = File::create(path.join("Cargo.toml"))?;
    cargo_file.write_all(cargo_body)?;
    Ok(())
}
