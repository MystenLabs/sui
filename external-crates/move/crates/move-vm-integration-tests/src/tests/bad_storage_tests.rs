// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::compiler::{as_module, compile_units, serialize_module_at_max_version};
use move_binary_format::errors::{Location, PartialVMError, VMError};
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, StructTag},
    resolver::{LinkageResolver, ModuleResolver, ResourceResolver},
    vm_status::{StatusCode, StatusType},
};
use move_vm_runtime::move_vm::MoveVM;
use move_vm_test_utils::InMemoryStorage;
use move_vm_types::gas::UnmeteredGasMeter;

const TEST_ADDR: AccountAddress = AccountAddress::new([42; AccountAddress::LENGTH]);

#[test]
fn test_malformed_module() {
    // Compile module M.
    let code = r#"
        module {{ADDR}}::M {
            public fun foo() {}
        }
    "#;

    let code = code.replace("{{ADDR}}", &format!("0x{}", TEST_ADDR));
    let mut units = compile_units(&code).unwrap();

    let m = as_module(units.pop().unwrap());

    let mut blob = vec![];
    serialize_module_at_max_version(&m, &mut blob).unwrap();

    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("M").unwrap());
    let fun_name = Identifier::new("foo").unwrap();

    // Publish M and call M::foo. No errors should be thrown.
    {
        let mut storage = InMemoryStorage::new();
        storage.publish_or_overwrite_module(m.self_id(), blob.clone());
        let vm = MoveVM::new(vec![]).unwrap();
        let mut sess = vm.new_session(&storage);
        sess.execute_function_bypass_visibility(
            &module_id,
            &fun_name,
            vec![],
            Vec::<Vec<u8>>::new(),
            &mut UnmeteredGasMeter,
            None,
        )
        .unwrap();
    }

    // Start over with a fresh storage and publish a corrupted version of M.
    // A fresh VM needs to be used whenever the storage has been modified or otherwise the
    // loader cache gets out of sync.
    //
    // Try to call M::foo again and the module should fail to load, causing an
    // invariant violation error.
    {
        blob[0] = 0xde;
        blob[1] = 0xad;
        blob[2] = 0xbe;
        blob[3] = 0xef;
        let mut storage = InMemoryStorage::new();
        storage.publish_or_overwrite_module(m.self_id(), blob);
        let vm = MoveVM::new(vec![]).unwrap();
        let mut sess = vm.new_session(&storage);
        let err = sess
            .execute_function_bypass_visibility(
                &module_id,
                &fun_name,
                vec![],
                Vec::<Vec<u8>>::new(),
                &mut UnmeteredGasMeter,
                None,
            )
            .unwrap_err();
        assert_eq!(err.status_type(), StatusType::InvariantViolation);
    }
}

#[test]
fn test_unverifiable_module() {
    // Compile module M.
    let code = r#"
        module {{ADDR}}::M {
            public fun foo() {}
        }
    "#;

    let code = code.replace("{{ADDR}}", &format!("0x{}", TEST_ADDR));
    let mut units = compile_units(&code).unwrap();
    let m = as_module(units.pop().unwrap());

    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("M").unwrap());
    let fun_name = Identifier::new("foo").unwrap();

    // Publish M and call M::foo to make sure it works.
    {
        let mut storage = InMemoryStorage::new();

        let mut blob = vec![];
        serialize_module_at_max_version(&m, &mut blob).unwrap();
        storage.publish_or_overwrite_module(m.self_id(), blob);

        let vm = MoveVM::new(vec![]).unwrap();
        let mut sess = vm.new_session(&storage);

        sess.execute_function_bypass_visibility(
            &module_id,
            &fun_name,
            vec![],
            Vec::<Vec<u8>>::new(),
            &mut UnmeteredGasMeter,
            None,
        )
        .unwrap();
    }

    // Erase the body of M::foo to make it fail verification.
    // Publish this modified version of M and the VM should fail to load it.
    {
        let mut storage = InMemoryStorage::new();

        let mut m = m;
        m.function_defs[0].code.as_mut().unwrap().code = vec![];
        let mut blob = vec![];
        serialize_module_at_max_version(&m, &mut blob).unwrap();
        storage.publish_or_overwrite_module(m.self_id(), blob);

        let vm = MoveVM::new(vec![]).unwrap();
        let mut sess = vm.new_session(&storage);

        let err = sess
            .execute_function_bypass_visibility(
                &module_id,
                &fun_name,
                vec![],
                Vec::<Vec<u8>>::new(),
                &mut UnmeteredGasMeter,
                None,
            )
            .unwrap_err();

        assert_eq!(err.status_type(), StatusType::InvariantViolation);
    }
}

#[test]
fn test_missing_module_dependency() {
    // Compile two modules M, N where N depends on M.
    let code = r#"
        module {{ADDR}}::M {
            public fun foo() {}
        }

        module {{ADDR}}::N {
            use {{ADDR}}::M;

            public fun bar() { M::foo(); }
        }
    "#;
    let code = code.replace("{{ADDR}}", &format!("0x{}", TEST_ADDR));
    let mut units = compile_units(&code).unwrap();
    let n = as_module(units.pop().unwrap());
    let m = as_module(units.pop().unwrap());

    let mut blob_m = vec![];
    serialize_module_at_max_version(&m, &mut blob_m).unwrap();
    let mut blob_n = vec![];
    serialize_module_at_max_version(&n, &mut blob_n).unwrap();

    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("N").unwrap());
    let fun_name = Identifier::new("bar").unwrap();

    // Publish M and N and call N::bar. Everything should work.
    {
        let mut storage = InMemoryStorage::new();

        storage.publish_or_overwrite_module(m.self_id(), blob_m);
        storage.publish_or_overwrite_module(n.self_id(), blob_n.clone());

        let vm = MoveVM::new(vec![]).unwrap();
        let mut sess = vm.new_session(&storage);

        sess.execute_function_bypass_visibility(
            &module_id,
            &fun_name,
            vec![],
            Vec::<Vec<u8>>::new(),
            &mut UnmeteredGasMeter,
            None,
        )
        .unwrap();
    }

    // Publish only N and try to call N::bar. The VM should fail to find M and raise
    // an invariant violation.
    {
        let mut storage = InMemoryStorage::new();
        storage.publish_or_overwrite_module(n.self_id(), blob_n);

        let vm = MoveVM::new(vec![]).unwrap();
        let mut sess = vm.new_session(&storage);

        let err = sess
            .execute_function_bypass_visibility(
                &module_id,
                &fun_name,
                vec![],
                Vec::<Vec<u8>>::new(),
                &mut UnmeteredGasMeter,
                None,
            )
            .unwrap_err();

        assert_eq!(err.status_type(), StatusType::InvariantViolation);
    }
}

#[test]
fn test_malformed_module_dependency() {
    // Compile two modules M, N where N depends on M.
    let code = r#"
        module {{ADDR}}::M {
            public fun foo() {}
        }

        module {{ADDR}}::N {
            use {{ADDR}}::M;

            public fun bar() { M::foo(); }
        }
    "#;
    let code = code.replace("{{ADDR}}", &format!("0x{}", TEST_ADDR));
    let mut units = compile_units(&code).unwrap();
    let n = as_module(units.pop().unwrap());
    let m = as_module(units.pop().unwrap());

    let mut blob_m = vec![];
    serialize_module_at_max_version(&m, &mut blob_m).unwrap();
    let mut blob_n = vec![];
    serialize_module_at_max_version(&n, &mut blob_n).unwrap();

    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("N").unwrap());
    let fun_name = Identifier::new("bar").unwrap();

    // Publish M and N and call N::bar. Everything should work.
    {
        let mut storage = InMemoryStorage::new();

        storage.publish_or_overwrite_module(m.self_id(), blob_m.clone());
        storage.publish_or_overwrite_module(n.self_id(), blob_n.clone());

        let vm = MoveVM::new(vec![]).unwrap();
        let mut sess = vm.new_session(&storage);

        sess.execute_function_bypass_visibility(
            &module_id,
            &fun_name,
            vec![],
            Vec::<Vec<u8>>::new(),
            &mut UnmeteredGasMeter,
            None,
        )
        .unwrap();
    }

    // Publish N and a corrupted version of M and try to call N::bar, the VM should fail to load M.
    {
        blob_m[0] = 0xde;
        blob_m[1] = 0xad;
        blob_m[2] = 0xbe;
        blob_m[3] = 0xef;

        let mut storage = InMemoryStorage::new();

        storage.publish_or_overwrite_module(m.self_id(), blob_m);
        storage.publish_or_overwrite_module(n.self_id(), blob_n);

        let vm = MoveVM::new(vec![]).unwrap();
        let mut sess = vm.new_session(&storage);

        let err = sess
            .execute_function_bypass_visibility(
                &module_id,
                &fun_name,
                vec![],
                Vec::<Vec<u8>>::new(),
                &mut UnmeteredGasMeter,
                None,
            )
            .unwrap_err();

        assert_eq!(err.status_type(), StatusType::InvariantViolation);
    }
}

#[test]
fn test_unverifiable_module_dependency() {
    // Compile two modules M, N where N depends on M.
    let code = r#"
        module {{ADDR}}::M {
            public fun foo() {}
        }

        module {{ADDR}}::N {
            use {{ADDR}}::M;

            public fun bar() { M::foo(); }
        }
    "#;
    let code = code.replace("{{ADDR}}", &format!("0x{}", TEST_ADDR));
    let mut units = compile_units(&code).unwrap();
    let n = as_module(units.pop().unwrap());
    let m = as_module(units.pop().unwrap());

    let mut blob_n = vec![];
    serialize_module_at_max_version(&n, &mut blob_n).unwrap();

    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("N").unwrap());
    let fun_name = Identifier::new("bar").unwrap();

    // Publish M and N and call N::bar. Everything should work.
    {
        let mut blob_m = vec![];
        serialize_module_at_max_version(&m, &mut blob_m).unwrap();

        let mut storage = InMemoryStorage::new();

        storage.publish_or_overwrite_module(m.self_id(), blob_m);
        storage.publish_or_overwrite_module(n.self_id(), blob_n.clone());

        let vm = MoveVM::new(vec![]).unwrap();
        let mut sess = vm.new_session(&storage);

        sess.execute_function_bypass_visibility(
            &module_id,
            &fun_name,
            vec![],
            Vec::<Vec<u8>>::new(),
            &mut UnmeteredGasMeter,
            None,
        )
        .unwrap();
    }

    // Publish N and an unverifiable version of M and try to call N::bar, the VM should fail to load M.
    {
        let mut m = m;
        m.function_defs[0].code.as_mut().unwrap().code = vec![];
        let mut blob_m = vec![];
        serialize_module_at_max_version(&m, &mut blob_m).unwrap();

        let mut storage = InMemoryStorage::new();

        storage.publish_or_overwrite_module(m.self_id(), blob_m);
        storage.publish_or_overwrite_module(n.self_id(), blob_n);

        let vm = MoveVM::new(vec![]).unwrap();
        let mut sess = vm.new_session(&storage);

        let err = sess
            .execute_function_bypass_visibility(
                &module_id,
                &fun_name,
                vec![],
                Vec::<Vec<u8>>::new(),
                &mut UnmeteredGasMeter,
                None,
            )
            .unwrap_err();

        assert_eq!(err.status_type(), StatusType::InvariantViolation);
    }
}

struct BogusStorage {
    bad_status_code: StatusCode,
}

impl LinkageResolver for BogusStorage {
    type Error = VMError;

    /// Don't do any relocation so module and resource loading can produce errors
    fn relocate(&self, module_id: &ModuleId) -> Result<ModuleId, Self::Error> {
        Ok(module_id.clone())
    }
}

impl ModuleResolver for BogusStorage {
    type Error = VMError;

    fn get_module(&self, _module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        Err(PartialVMError::new(self.bad_status_code).finish(Location::Undefined))
    }
}

impl ResourceResolver for BogusStorage {
    type Error = VMError;

    fn get_resource(
        &self,
        _address: &AccountAddress,
        _tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        Err(PartialVMError::new(self.bad_status_code).finish(Location::Undefined))
    }
}

const LIST_OF_ERROR_CODES: &[StatusCode] = &[
    StatusCode::UNKNOWN_VALIDATION_STATUS,
    StatusCode::INVALID_SIGNATURE,
    StatusCode::UNKNOWN_VERIFICATION_ERROR,
    StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR,
    StatusCode::UNKNOWN_BINARY_ERROR,
    StatusCode::UNKNOWN_RUNTIME_STATUS,
    StatusCode::UNKNOWN_STATUS,
];

#[test]
fn test_storage_returns_bogus_error_when_loading_module() {
    let module_id = ModuleId::new(TEST_ADDR, Identifier::new("N").unwrap());
    let fun_name = Identifier::new("bar").unwrap();

    for error_code in LIST_OF_ERROR_CODES {
        let storage = BogusStorage {
            bad_status_code: *error_code,
        };
        let vm = MoveVM::new(vec![]).unwrap();
        let mut sess = vm.new_session(&storage);

        let err = sess
            .execute_function_bypass_visibility(
                &module_id,
                &fun_name,
                vec![],
                Vec::<Vec<u8>>::new(),
                &mut UnmeteredGasMeter,
                None,
            )
            .unwrap_err();

        assert_eq!(err.status_type(), StatusType::InvariantViolation);
    }
}
