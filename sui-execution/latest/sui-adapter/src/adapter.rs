// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;
#[sui_macros::with_checked_arithmetic]
mod checked {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::{collections::BTreeMap, sync::Arc};

    use anyhow::Result;
    use move_binary_format::file_format::CompiledModule;
    use move_core_types::account_address::AccountAddress;
    use move_vm_config::runtime::{VMConfig, VMRuntimeLimitsConfig};
    use move_vm_runtime::{
        move_vm::MoveVM, native_extensions::NativeContextExtensions,
        native_functions::NativeFunctionTable,
    };
    use sui_move_natives::{object_runtime, transaction_context::TransactionContext};

    use sui_move_natives::{NativesCostTable, object_runtime::ObjectRuntime};
    use sui_protocol_config::ProtocolConfig;
    use sui_types::{
        base_types::*,
        error::ExecutionError,
        error::{ExecutionErrorKind, SuiError},
        execution_config_utils::to_binary_config,
        metrics::LimitsMetrics,
        storage::ChildObjectResolver,
    };

    pub fn new_move_vm(
        natives: NativeFunctionTable,
        protocol_config: &ProtocolConfig,
    ) -> Result<MoveVM, SuiError> {
        MoveVM::new_with_config(
            natives,
            VMConfig {
                verifier: protocol_config.verifier_config(/* signing_limits */ None),
                max_binary_format_version: protocol_config.move_binary_format_version(),
                runtime_limits_config: VMRuntimeLimitsConfig {
                    vector_len_max: protocol_config.max_move_vector_len(),
                    max_value_nest_depth: protocol_config.max_move_value_depth_as_option(),
                    hardened_otw_check: protocol_config.hardened_otw_check(),
                },
                enable_invariant_violation_check_in_swap_loc: !protocol_config
                    .disable_invariant_violation_check_in_swap_loc(),
                check_no_extraneous_bytes_during_deserialization: protocol_config
                    .no_extraneous_module_bytes(),
                // Don't augment errors with execution state on-chain
                error_execution_state: false,
                binary_config: to_binary_config(protocol_config),
                rethrow_serialization_type_layout_errors: protocol_config
                    .rethrow_serialization_type_layout_errors(),
                max_type_to_layout_nodes: protocol_config.max_type_to_layout_nodes_as_option(),
                variant_nodes: protocol_config.variant_nodes(),
            },
        )
        .map_err(|_| SuiError::ExecutionInvariantViolation)
    }

    pub fn new_native_extensions<'r>(
        child_resolver: &'r dyn ChildObjectResolver,
        input_objects: BTreeMap<ObjectID, object_runtime::InputObject>,
        is_metered: bool,
        protocol_config: &'r ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        tx_context: Rc<RefCell<TxContext>>,
    ) -> NativeContextExtensions<'r> {
        let current_epoch_id: EpochId = tx_context.borrow().epoch();
        let mut extensions = NativeContextExtensions::default();
        extensions.add(ObjectRuntime::new(
            child_resolver,
            input_objects,
            is_metered,
            protocol_config,
            metrics,
            current_epoch_id,
        ));
        extensions.add(NativesCostTable::from_protocol_config(protocol_config));
        extensions.add(TransactionContext::new(tx_context));
        extensions
    }

    /// Given a list of `modules` and an `object_id`, mutate each module's self ID (which must be
    /// 0x0) to be `object_id`.
    pub fn substitute_package_id(
        modules: &mut [CompiledModule],
        object_id: ObjectID,
    ) -> Result<(), ExecutionError> {
        let new_address = AccountAddress::from(object_id);

        for module in modules.iter_mut() {
            let self_handle = module.self_handle().clone();
            let self_address_idx = self_handle.address;

            let addrs = &mut module.address_identifiers;
            let Some(address_mut) = addrs.get_mut(self_address_idx.0 as usize) else {
                let name = module.identifier_at(self_handle.name);
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::PublishErrorNonZeroAddress,
                    format!("Publishing module {name} with invalid address index"),
                ));
            };

            if *address_mut != AccountAddress::ZERO {
                let name = module.identifier_at(self_handle.name);
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::PublishErrorNonZeroAddress,
                    format!("Publishing module {name} with non-zero address is not allowed"),
                ));
            };

            *address_mut = new_address;
        }

        Ok(())
    }

    pub fn missing_unwrapped_msg(id: &ObjectID) -> String {
        format!(
            "Unable to unwrap object {}. Was unable to retrieve last known version in the parent sync",
            id
        )
    }
}
