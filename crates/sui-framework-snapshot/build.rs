// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::path::Path;

include!("src/manifest.rs");

/// Output a file `OUT_DIR/framework_manifest.rs` containing the contents of the manifest as a
/// rust literal of type `[(u64, SingleSnapshot)]`.
fn generate_framework_version_table() -> anyhow::Result<()> {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("framework_manifest.rs");

    let manifest = load_bytecode_snapshot_manifest();
    let manifest_path = manifest_path().to_string_lossy().into_owned();

    let mut file = BufWriter::new(File::create(&dest_path)?);
    writeln!(&mut file, "// (protocol version, framework info)")?;

    writeln!(&mut file, "[")?;

    for (version, entry) in manifest.iter() {
        let hash = entry.git_revision();
        writeln!(&mut file, "  ({version}, SingleSnapshot {{")?;
        writeln!(&mut file, "        git_revision: \"{hash}\".into(),")?;
        writeln!(&mut file, "        package_ids: [")?;
        for oid in entry.package_ids() {
            writeln!(
                &mut file,
                "          ObjectID::from_hex_literal(\"{oid}\").unwrap(),"
            )?;
        }
        writeln!(&mut file, "        ].into(),")?;
        writeln!(&mut file, "      }}),")?;
    }

    writeln!(&mut file, "]")?;

    println!("cargo::rerun-if-changed={}", manifest_path);
    println!("cargo::rerun-if-changed=build.rs");
    println!("cargo::rerun-if-changed=src/manifest.rs");
    Ok(())
}

fn main() {
    generate_framework_version_table().unwrap();
}
