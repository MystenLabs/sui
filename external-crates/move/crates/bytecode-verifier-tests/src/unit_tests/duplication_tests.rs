// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::*;
use move_bytecode_verifier::DuplicationChecker;

#[test]
fn duplicated_friend_decls() {
    let mut m = basic_test_module();
    let handle = ModuleHandle {
        address: AddressIdentifierIndex::new(0),
        name: IdentifierIndex::new(0),
    };
    m.friend_decls.push(handle.clone());
    m.friend_decls.push(handle);
    DuplicationChecker::verify_module(&m).unwrap_err();
}

#[test]
fn duplicated_variant_handles() {
    let mut m = basic_test_module_with_enum();
    m.variant_handles.push(m.variant_handles[0].clone());
    DuplicationChecker::verify_module(&m).unwrap_err();
}
