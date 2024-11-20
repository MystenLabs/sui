// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use clap::Parser;
use move_cli::base::new;
use move_package::source_package::layout::SourcePackageLayout;
use std::{fs::create_dir_all, io::Write, path::Path};

const SUI_PKG_NAME: &str = "Sui";

// Use testnet by default. Probably want to add options to make this configurable later
const SUI_PKG_PATH: &str = "{ git = \"https://github.com/MystenLabs/sui.git\", subdir = \"crates/sui-framework/packages/sui-framework\", rev = \"framework/testnet\" }";

#[derive(Parser)]
#[group(id = "sui-move-new")]
pub struct New {
    #[clap(flatten)]
    pub new: new::New,
}

impl New {
    pub fn execute(self, path: Option<&Path>) -> anyhow::Result<()> {
        let name = &self.new.name.to_lowercase();

        self.new
            .execute(path, [(SUI_PKG_NAME, SUI_PKG_PATH)], [(name, "0x0")], "")?;
        let p = path.unwrap_or_else(|| Path::new(&name));
        let mut w = std::fs::File::create(
            p.join(SourcePackageLayout::Sources.path())
                .join(format!("{name}.move")),
        )?;
        writeln!(
            w,
            r#"/*
/// Module: {name}
module {name}::{name};
*/

// The following recommendations are based on 2024 Move.

// === Imports ===

// === Errors ===
// Use PascalCase for errors, start with an E and be descriptive.
// ex: const ENameHasMaxLengthOf64Chars: u64 = 0;
// https://docs.sui.io/concepts/sui-move-concepts/conventions#errors

// === Constants ===

// === Structs ===
// * Describe the properties of your structs.
// https://docs.sui.io/concepts/sui-move-concepts/conventions#struct-property-comments
//
// * Do not use 'potato' in the name of structs. The lack of abilities define it as a potato pattern.
// https://docs.sui.io/concepts/sui-move-concepts/conventions#potato-structs

// === Method Aliases ===

// === Public-Mutative Functions ===
// * Name the functions that create data structures as `public fun empty`.
// https://docs.sui.io/concepts/sui-move-concepts/conventions#empty-function
//
// * Name the functions that create objects as `pub fun new`.
// https://docs.sui.io/concepts/sui-move-concepts/conventions#new-function
//
// * Library modules that share objects should provide two functions:
// one to create the object `public fun new(ctx:&mut TxContext): Object`
// and another to share it `public fun share(profile: Profile)`.
// It allows the caller to access its UID and run custom functionality before sharing it.
// https://docs.sui.io/concepts/sui-move-concepts/conventions#new-function
//
// * Name the functions that return a reference as `<PROPERTY-NAME>_mut`, replacing with
// <PROPERTY-NAME\> the actual name of the property.
// https://docs.sui.io/concepts/sui-move-concepts/conventions#reference-functions
//
// * Provide functions to delete objects. Destroy empty objects with the `public fun destroy_empty`
// Use the `public fun drop` for objects that have types that can be dropped.
// https://docs.sui.io/concepts/sui-move-concepts/conventions#destroy-functions
//
// * CRUD functions names
// `add`, `new`, `drop`, `empty`, `remove`, `destroy_empty`, `to_object_name`, `from_object_name`, `property_name_mut`
// https://docs.sui.io/concepts/sui-move-concepts/conventions#crud-functions-names

// === Public-View Functions ===
// * Name the functions that return a reference as <<PROPERTY-NAME>, replacing with
// <PROPERTY-NAME\> the actual name of the property.
// https://docs.sui.io/concepts/sui-move-concepts/conventions#reference-functions
//
// * Keep your functions pure to maintain composability. Do not use `transfer::transfer` or
// `transfer::public_transfer` inside core functions.
// https://docs.sui.io/concepts/sui-move-concepts/conventions#pure-functions
//
// * CRUD functions names
// `exists_`, `contains`, `property_name`
// https://docs.sui.io/concepts/sui-move-concepts/conventions#crud-functions-names

// === Admin Functions ===
// * In admin-gated functions, the first parameter should be the capability. It helps the autocomplete with user types.
// https://docs.sui.io/concepts/sui-move-concepts/conventions#admin-capability
//
// * To maintain composability, use capabilities instead of addresses for access control.
// https://docs.sui.io/concepts/sui-move-concepts/conventions#access-control

// === Public-Package Functions ===

// === Private Functions ===

// === Test Functions ===
"#,
            name = name
        )?;

        create_dir_all(p.join(SourcePackageLayout::Tests.path()))?;
        let mut w = std::fs::File::create(
            p.join(SourcePackageLayout::Tests.path())
                .join(format!("{name}_tests.move")),
        )?;
        writeln!(
            w,
            r#"/*
#[test_only]
module {name}::{name}_tests;
// uncomment this line to import the module
// use {name}::{name};

const ENotImplemented: u64 = 0;

#[test]
fun test_{name}() {{
    // pass
}}

#[test, expected_failure(abort_code = ::{name}::{name}_tests::ENotImplemented)]
fun test_{name}_fail() {{
    abort ENotImplemented
}}
*/"#,
            name = name
        )?;

        Ok(())
    }
}
