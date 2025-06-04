// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! This module defines the shared environment, `Env`, used for the compilation/translation and
//! execution of programmable transactions. While the "context" for each pass might be different,
//! the `Env` provides consistent access to shared components such as the VM or the protocol config.

use super::LinkageView;
use crate::{
    execution_value::ExecutionState,
    programmable_transactions::data_store::SuiDataStore,
    static_programmable_transactions::loading::ast::{self as L, LoadedFunction, Type},
};
use move_binary_format::{
    CompiledModule,
    errors::VMError,
    file_format::{AbilitySet, TypeParameterIndex},
};
use move_core_types::{
    annotated_value,
    language_storage::{ModuleId, StructTag},
    runtime_value,
};
use move_vm_runtime::move_vm::MoveVM;
use std::{cell::OnceCell, rc::Rc, sync::Arc};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    Identifier,
    base_types::ObjectID,
    error::{ExecutionError, ExecutionErrorKind},
    gas_coin::GasCoin,
    move_package::{UpgradeReceipt, UpgradeTicket},
    object::Object,
    type_input::TypeInput,
};

pub struct Env<'pc, 'vm, 'state, 'linkage> {
    pub protocol_config: &'pc ProtocolConfig,
    pub vm: &'vm MoveVM,
    pub state_view: &'state mut dyn ExecutionState,
    pub linkage_view: &'linkage LinkageView<'state>,
    gas_coin_type: OnceCell<Type>,
    upgrade_ticket_type: OnceCell<Type>,
    upgrade_receipt_type: OnceCell<Type>,
}

macro_rules! get_or_init_ty {
    ($env:expr, $ident:ident, $tag:expr) => {{
        let env = $env;
        if env.$ident.get().is_none() {
            let tag = $tag;
            let ty = env.load_type_from_struct(&tag)?;
            env.$ident.set(ty.clone()).unwrap();
        }
        Ok(env.$ident.get().unwrap().clone())
    }};
}

impl<'pc, 'vm, 'state, 'linkage> Env<'pc, 'vm, 'state, 'linkage> {
    pub fn new(
        protocol_config: &'pc ProtocolConfig,
        vm: &'vm MoveVM,
        state_view: &'state mut dyn ExecutionState,
        linkage_view: &'linkage LinkageView<'state>,
    ) -> Self {
        Self {
            protocol_config,
            vm,
            state_view,
            linkage_view,
            gas_coin_type: OnceCell::new(),
            upgrade_ticket_type: OnceCell::new(),
            upgrade_receipt_type: OnceCell::new(),
        }
    }

    pub fn convert_vm_error(&self, e: VMError) -> ExecutionError {
        crate::error::convert_vm_error(
            e,
            self.vm,
            self.linkage_view,
            self.protocol_config.resolve_abort_locations_to_package_id(),
        )
    }

    pub fn convert_type_argument_error(&self, idx: usize, e: VMError) -> ExecutionError {
        use move_core_types::vm_status::StatusCode;
        use sui_types::execution_status::TypeArgumentError;
        match e.major_status() {
            StatusCode::NUMBER_OF_TYPE_ARGUMENTS_MISMATCH => {
                ExecutionErrorKind::TypeArityMismatch.into()
            }
            StatusCode::TYPE_RESOLUTION_FAILURE => ExecutionErrorKind::TypeArgumentError {
                argument_idx: idx as TypeParameterIndex,
                kind: TypeArgumentError::TypeNotFound,
            }
            .into(),
            StatusCode::CONSTRAINT_NOT_SATISFIED => ExecutionErrorKind::TypeArgumentError {
                argument_idx: idx as TypeParameterIndex,
                kind: TypeArgumentError::ConstraintNotSatisfied,
            }
            .into(),
            _ => self.convert_vm_error(e),
        }
    }

    pub fn module_definition(
        &self,
        module_id: &ModuleId,
    ) -> Result<Arc<CompiledModule>, ExecutionError> {
        let data_store = SuiDataStore::new(self.linkage_view, &[]);
        self.vm
            .get_runtime()
            .load_module(module_id, &data_store)
            .map_err(|e| self.convert_vm_error(e))
    }

    pub fn fully_annotated_layout(
        &self,
        _ty: &Type,
    ) -> Result<annotated_value::MoveTypeLayout, ExecutionError> {
        todo!("LOADING")
    }

    pub fn runtime_layout(
        &self,
        _ty: &Type,
    ) -> Result<runtime_value::MoveTypeLayout, ExecutionError> {
        todo!("LOADING")
    }

    pub fn load_function(
        &self,
        _package: ObjectID,
        _module: String,
        _function: String,
        _type_arguments: Vec<Type>,
    ) -> Result<LoadedFunction, ExecutionError> {
        // assert_invariant!(
        //     self.linkage_view.has_linkage(package)?,
        //     "packages need to be linked before typing"
        // );
        // let package_address: AccountAddress = package.into();
        // let original_address = self
        //     .linkage_view
        //     .original_package_id()?
        //     .unwrap_or(package_address);
        // let module = to_identifier(module)?;
        // let name = to_identifier(function)?;
        // let storage_id = ModuleId::new(package_address, module.clone());
        // let runtime_id = ModuleId::new(original_address, module);
        // let mut data_store = SuiDataStore::new(self.linkage_view, &[]);
        // let signature = self
        //     .vm
        //     .get_runtime()
        //     .load_function(
        //         &runtime_id,
        //         name.as_ident_str(),
        //         &type_arguments,
        //         &mut data_store,
        //     )
        //     .map_err(|e| self.convert_vm_error(e))?;
        // let tx_context = match signature.parameters.last() {
        //     Some(t) => is_tx_context(self, t)?,
        //     None => TxContextKind::None,
        // };
        // Ok(ast::LoadedFunction {
        //     storage_id,
        //     runtime_id,
        //     name,
        //     type_arguments,
        //     signature,
        //     tx_context,
        // })
        todo!("LOADING")
    }

    pub fn load_type_input(&self, _idx: usize, _ty: TypeInput) -> Result<Type, ExecutionError> {
        // let tag = ty
        //     .into_type_tag()
        //     .map_err(|e| make_invariant_violation!("{}", e.to_string()))?;
        // load_type(self.vm, self.linkage_view, &[], &tag)
        //     .map_err(|e| self.convert_type_argument_error(idx, e))
        todo!("LOADING")
    }

    pub fn load_type_from_struct(&self, _tag: &StructTag) -> Result<Type, ExecutionError> {
        // load_type_from_struct(self.vm, self.linkage_view, &[], tag)
        //     .map_err(|e| self.convert_vm_error(e))
        todo!("LOADING")
    }

    pub fn gas_coin_type(&self) -> Result<Type, ExecutionError> {
        get_or_init_ty!(self, gas_coin_type, GasCoin::type_())
    }

    pub fn upgrade_ticket_type(&self) -> Result<Type, ExecutionError> {
        get_or_init_ty!(self, upgrade_ticket_type, UpgradeTicket::type_())
    }

    pub fn upgrade_receipt_type(&self) -> Result<Type, ExecutionError> {
        get_or_init_ty!(self, upgrade_receipt_type, UpgradeReceipt::type_())
    }

    pub fn vector_type(&self, element_type: Type) -> Result<Type, ExecutionError> {
        let abilities = AbilitySet::polymorphic_abilities(
            AbilitySet::VECTOR,
            [false],
            [element_type.abilities()],
        )
        .map_err(|e| {
            ExecutionError::new_with_source(ExecutionErrorKind::VMInvariantViolation, e.to_string())
        })?;
        Ok(Type::Vector(Rc::new(L::Vector {
            abilities,
            element_type,
        })))
    }

    pub fn read_object(&self, id: &ObjectID) -> Result<&Object, ExecutionError> {
        let Some(obj) = self.state_view.read_object(id) else {
            // protected by transaction input checker
            invariant_violation!("Object {:?} does not exist", id);
        };
        Ok(obj)
    }
}

#[allow(unused)]
fn to_identifier(name: String) -> Result<Identifier, ExecutionError> {
    Identifier::new(name).map_err(|e| {
        ExecutionError::new_with_source(ExecutionErrorKind::VMInvariantViolation, e.to_string())
    })
}
