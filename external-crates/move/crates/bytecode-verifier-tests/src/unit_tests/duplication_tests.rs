// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::*;
use move_bytecode_verifier::DuplicationChecker;
use proptest::prelude::*;

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

proptest! {
    #[test]
    fn valid_duplication(module in CompiledModule::valid_strategy(20)) {
        // println!("{:#?}", module);
        let x = DuplicationChecker::verify_module(&module);
        println!("{:#?}", x);
        prop_assert!(x.is_ok());
    }
}
