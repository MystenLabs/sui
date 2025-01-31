// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{env, fs::File};
use std::{
    io::{BufWriter, Write},
    path::Path,
};
use sui_framework_snapshot::{load_bytecode_snapshot_manifest, manifest_path};

/// Output a file `OUT_DIR/version_table.rs` containing the contents of the manifest as a
/// rust literal of type `[(u64, FrameworkVersion)]`.
fn generate_framework_version_table() -> anyhow::Result<()> {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("version_table.rs");

    let manifest = load_bytecode_snapshot_manifest();
    let manifest_path = manifest_path().to_string_lossy().into_owned();

    let mut file = BufWriter::new(File::create(&dest_path)?);

    writeln!(&mut file, "[")?;

    for (version, entry) in manifest.iter() {
        let hash = entry.git_revision();
        writeln!(
            &mut file,
            "  (ProtocolVersion::new( {version:>2} ), FrameworkVersion {{"
        )?;
        writeln!(&mut file, "        git_revision: \"{hash}\".into(),")?;
        writeln!(&mut file, "        framework_package_names: [")?;
        for _oid in entry.package_ids() {
            writeln!(&mut file, "          \"TODO\".to_string(),")?;
        }
        writeln!(&mut file, "        ].into(),")?;
        writeln!(&mut file, "      }}),")?;
    }

    writeln!(&mut file, "]")?;

    println!("cargo::rerun-if-changed={}", manifest_path);
    println!("cargo::rerun-if-changed=build.rs");
    Ok(())
}

fn main() {
    generate_framework_version_table().unwrap();
}
