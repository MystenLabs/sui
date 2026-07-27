// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! `public(package)` constants in pre-compiled modules can be used from source modules of the
//! same package: their folded values are seeded from the pre-compiled program info, so they fold
//! into constant definitions and are copied into function bodies like any other constant. This is
//! the analyzer's interaction pattern -- it compiles against cached pre-compiled dependencies --
//! and is not reachable in the move_check testsuite, which always compiles dependencies from
//! source.

use move_compiler::{
    Compiler,
    command_line::compiler::construct_pre_compiled_lib,
    diagnostics::report_diagnostics_to_buffer,
    editions::Edition,
    shared::{Flags, NumericalAddress, PackageConfig, PackagePaths},
};
use std::{collections::BTreeMap, sync::Arc};
use vfs::{VfsPath, impls::memory::MemoryFS};

const LIB: &str = r#"
module a::m {
    public(package) const MAX: u64 = 100;
    public(package) const BYTES: vector<u8> = b"hello";
    public(package) const ADDR: address = @0x7;
}
"#;

const USER: &str = r#"
module a::n {
    use a::m;

    const D: u64 = m::MAX + 1;

    public fun read(): u64 { m::MAX + D }

    public fun bytes(): vector<u8> { m::BYTES }

    public fun addr(): address { m::ADDR }
}
"#;

fn config() -> PackageConfig {
    PackageConfig {
        edition: Edition::E2024_ALPHA,
        ..PackageConfig::default()
    }
}

fn package_paths(paths: &[&str]) -> Vec<PackagePaths> {
    vec![PackagePaths {
        name: Some(("pkg".into(), config())),
        paths: paths.iter().map(|p| (*p).into()).collect(),
        named_address_map: BTreeMap::from([(
            "a".into(),
            NumericalAddress::parse_str("0x42").unwrap(),
        )]),
    }]
}

fn sources_vfs() -> VfsPath {
    let vfs_root = VfsPath::new(MemoryFS::new());
    for (path, source) in [("lib.move", LIB), ("user.move", USER)] {
        vfs_root
            .join(path)
            .unwrap()
            .create_file()
            .unwrap()
            .write_all(source.as_bytes())
            .unwrap();
    }
    vfs_root
}

fn expect_units(
    (files, res): (
        move_compiler::shared::files::MappedFiles,
        Result<
            (
                Vec<move_compiler::compiled_unit::AnnotatedCompiledUnit>,
                move_compiler::diagnostics::Diagnostics,
            ),
            move_compiler::diagnostics::Diagnostics,
        >,
    ),
) -> Vec<move_compiler::compiled_unit::AnnotatedCompiledUnit> {
    match res {
        Ok((units, warnings)) => {
            let rendered = String::from_utf8(report_diagnostics_to_buffer(
                &files, warnings, /* color */ false,
            ))
            .unwrap();
            assert!(rendered.is_empty(), "{rendered}");
            units
        }
        Err(diags) => {
            let rendered = String::from_utf8(report_diagnostics_to_buffer(
                &files, diags, /* color */ false,
            ))
            .unwrap();
            panic!("{rendered}");
        }
    }
}

fn constant_names(unit: &move_compiler::compiled_unit::AnnotatedCompiledUnit) -> Vec<&str> {
    unit.named_module
        .source_map
        .constant_map
        .keys()
        .map(|c| c.0.as_str())
        .collect()
}

#[test]
fn cross_module_constants_from_pre_compiled_lib() {
    let vfs_root = sources_vfs();
    let lib = construct_pre_compiled_lib(
        package_paths(&["lib.move"]),
        None,
        None,
        /* interface_only */ false,
        Flags::empty(),
        Some(vfs_root.clone()),
    )
    .unwrap()
    .unwrap_or_else(|_| panic!("pre-compiled lib should compile without errors"));

    let units = expect_units(
        Compiler::from_package_paths(
            Some(vfs_root),
            package_paths(&["user.move"]),
            Vec::<PackagePaths>::new(),
        )
        .unwrap()
        .set_pre_compiled_program_opt(Some(Arc::new(lib)))
        .build()
        .unwrap(),
    );

    // the user module's source map names its own constant and the synthesized copies of the
    // pre-compiled module's constants
    let [user] = units.as_slice() else {
        panic!("expected exactly the user module to be compiled");
    };
    assert_eq!(user.named_module.name.as_str(), "n");
    assert_eq!(
        constant_names(user),
        vec!["D", "_p0_m_ADDR", "_p0_m_BYTES", "_p0_m_MAX"]
    );
}

#[test]
fn cross_module_constant_copies_in_source_map() {
    let vfs_root = sources_vfs();
    let units = expect_units(
        Compiler::from_package_paths(
            Some(vfs_root),
            package_paths(&["lib.move", "user.move"]),
            Vec::<PackagePaths>::new(),
        )
        .unwrap()
        .build()
        .unwrap(),
    );

    // the defining module's source map is unchanged, and the user module's names the copies
    // under the defining module's dependency order
    let [lib, user] = units.as_slice() else {
        panic!("expected the lib and user modules to be compiled");
    };
    assert_eq!(lib.named_module.name.as_str(), "m");
    assert_eq!(constant_names(lib), vec!["ADDR", "BYTES", "MAX"]);
    assert_eq!(user.named_module.name.as_str(), "n");
    assert_eq!(
        constant_names(user),
        vec!["D", "_0_m_ADDR", "_0_m_BYTES", "_0_m_MAX"]
    );
}
