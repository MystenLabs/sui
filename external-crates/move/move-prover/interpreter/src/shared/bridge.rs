// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::{PartialVMError, VMResult};
use move_core_types::{
    effects::{AccountChangeSet, ChangeSet, Op},
    resolver::MoveResolver,
};

/// The result returned by the stackless VM does not contain code offsets and indices. In order to
/// do cross-vm comparison, we need to adapt the Move VM result by removing these fields.
pub fn adapt_move_vm_result<T>(result: VMResult<T>) -> VMResult<T> {
    result.map_err(|err| {
        let (status_code, sub_status, _, _, location, _, _) = err.all_data();
        let adapted = PartialVMError::new(status_code);
        let adapted = match sub_status {
            None => adapted,
            Some(status_code) => adapted.with_sub_status(status_code),
        };
        adapted.finish(location)
    })
}

/// The change-set produced by the stackless VM guarantees that for a global resource, if the
/// underlying value is not changed in the execution, there will not be an entry in the change set.
/// The same guarantee is not provided by the Move VM. In Move VM, we could borrow_global_mut but
/// write the same value back instead of an updated value. In this case, the Move VM produces an
/// entry in the change_set.
pub fn adapt_move_vm_change_set<S: MoveResolver>(
    change_set_result: VMResult<ChangeSet>,
    old_storage: &S,
) -> VMResult<ChangeSet> {
    change_set_result.map(|change_set| adapt_move_vm_change_set_internal(change_set, old_storage))
}

fn adapt_move_vm_change_set_internal<S: MoveResolver>(
    change_set: ChangeSet,
    old_storage: &S,
) -> ChangeSet {
    let mut adapted = ChangeSet::new();
    for (addr, state) in change_set.into_inner() {
        let (modules, resources) = state.into_inner();

        let resources = resources
            .into_iter()
            .filter(|(tag, op)| match op {
                Op::New(_) | Op::Delete => true,
                Op::Modify(new_val) => match old_storage.get_resource(&addr, tag).unwrap() {
                    Some(old_val) => new_val != &old_val,
                    None => true,
                },
            })
            .collect();

        adapted
            .add_account_changeset(
                addr,
                AccountChangeSet::from_modules_resources(modules, resources),
            )
            .unwrap();
    }
    adapted
}
