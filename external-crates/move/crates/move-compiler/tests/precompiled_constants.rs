// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Constants in pre-compiled modules cannot be used cross-module: each use -- in a function body
//! or a constant definition -- reports an error. This path is not reachable in the move_check
//! testsuite, which always compiles dependencies from source.

use move_compiler::{
    Compiler, PASS_PARSER,
    command_line::compiler::{construct_pre_compiled_lib, move_check_for_errors},
    diagnostics::report_diagnostics_to_buffer,
    editions::Edition,
    shared::{Flags, NumericalAddress, PackageConfig, PackagePaths},
};
use std::{collections::BTreeMap, sync::Arc};
use vfs::{VfsPath, impls::memory::MemoryFS};

const LIB: &str = r#"
module a::m {
    public(package) const MAX: u64 = 100;
}
"#;

const USER: &str = r#"
module a::n {
    use a::m;

    const D: u64 = m::MAX + 1;

    public fun read(): u64 { m::MAX + D }
}
"#;

fn config() -> PackageConfig {
    PackageConfig {
        edition: Edition::E2024_ALPHA,
        ..PackageConfig::default()
    }
}

fn package_paths(path: &str) -> Vec<PackagePaths> {
    vec![PackagePaths {
        name: Some(("pkg".into(), config())),
        paths: vec![path.into()],
        named_address_map: BTreeMap::from([(
            "a".into(),
            NumericalAddress::parse_str("0x42").unwrap(),
        )]),
    }]
}

#[test]
fn cross_module_constants_from_pre_compiled_lib() {
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

    let lib = construct_pre_compiled_lib(
        package_paths("lib.move"),
        None,
        None,
        /* interface_only */ false,
        Flags::empty(),
        Some(vfs_root.clone()),
    )
    .unwrap()
    .unwrap_or_else(|_| panic!("pre-compiled lib should compile without errors"));

    let (files, res) = Compiler::from_package_paths(
        Some(vfs_root),
        package_paths("user.move"),
        Vec::<PackagePaths>::new(),
    )
    .unwrap()
    .set_pre_compiled_program_opt(Some(Arc::new(lib)))
    .run::<PASS_PARSER>()
    .unwrap();
    let diags = move_check_for_errors(res);
    let rendered = String::from_utf8(report_diagnostics_to_buffer(
        &files, diags, /* color */ false,
    ))
    .unwrap();
    let expected = "Constants defined in modules outside of the current compilation cannot be \
                    accessed from other modules";
    assert_eq!(rendered.matches(expected).count(), 2, "{rendered}");
}
