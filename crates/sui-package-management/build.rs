// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use serde::Deserialize;
use std::collections::BTreeMap;
use std::{env, fs::File};
use std::{
    io::{BufWriter, Write},
    path::Path,
};

// Mirrors the subset of `sui_framework_snapshot::Snapshot` used here. Depending on
// sui-framework-snapshot from a build script compiles sui-types and its dependencies a second
// time for the host.
#[derive(Deserialize)]
struct Snapshot {
    git_revision: String,
    packages: Vec<SnapshotPackage>,
}

#[derive(Deserialize)]
struct SnapshotPackage {
    name: String,
    path: String,
}

/// Output a file `OUT_DIR/system_packages_version_table.rs` containing the contents of the manifest as a
/// rust literal of type `[(ProtocolVersion, SystemPackages)]`. This is included as the
/// static [system_packaes::VERSION_TABLE]
fn generate_system_packages_version_table() -> anyhow::Result<()> {
    let out_dir = env::var_os("OUT_DIR").unwrap();
    let dest_path = Path::new(&out_dir).join("system_packages_version_table.rs");

    let manifest_path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../sui-framework-snapshot/manifest.json")
        .to_string_lossy()
        .into_owned();
    let manifest: BTreeMap<u64, Snapshot> = match std::fs::read(&manifest_path) {
        Ok(bytes) => serde_json::from_slice(&bytes)?,
        Err(_) => BTreeMap::new(),
    };

    let mut file = BufWriter::new(File::create(&dest_path)?);

    writeln!(&mut file, "[")?;

    for (version, entry) in manifest.iter() {
        let hash = &entry.git_revision;
        writeln!(
            &mut file,
            "  (ProtocolVersion::new( {version:>2} ), SystemPackagesVersion {{"
        )?;
        writeln!(&mut file, "        git_revision: \"{hash}\".into(),")?;
        writeln!(&mut file, "        packages: [")?;
        for package in entry.packages.iter() {
            writeln!(
                &mut file,
                "          SystemPackage {{ package_name: \"{}\".into(), repo_path: \"{}\".into() }},",
                package.name, package.path,
            )?;
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
    generate_system_packages_version_table().unwrap();
}
