// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::{ast, LinkageView};
use crate::{
    execution_value::ExecutionState,
    programmable_transactions::context::{load_type, load_type_from_struct, SuiDataStore},
};
use move_binary_format::{errors::VMError, file_format::AbilitySet};
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, StructTag},
};
use move_vm_runtime::move_vm::MoveVM;
use move_vm_types::loaded_data::runtime_types::{CachedDatatype, CachedTypeIndex, Type};
use std::{cell::OnceCell, sync::Arc};
use sui_protocol_config::ProtocolConfig;
use sui_types::{
    base_types::ObjectID,
    error::{ExecutionError, ExecutionErrorKind},
    gas_coin::GasCoin,
    move_package::{UpgradeReceipt, UpgradeTicket},
    object::Object,
    type_input::TypeInput,
    Identifier, TypeTag,
};

pub struct Env<'a, 'b, 'state> {
    protocol_config: &'a ProtocolConfig,
    vm: &'a MoveVM,
    state_view: &'a dyn ExecutionState,
    linkage_view: &'b LinkageView<'state>,
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
        Ok(env.$ident.get().unwrap())
    }};
}

impl<'a, 'b, 'state> Env<'a, 'b, 'state> {
    pub fn new(
        protocol_config: &'a ProtocolConfig,
        vm: &'a MoveVM,
        state_view: &'a dyn ExecutionState,
        linkage_view: &'b LinkageView<'state>,
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

    pub fn load_function(
        &self,
        package: ObjectID,
        module: String,
        function: String,
        type_arguments: Vec<Type>,
    ) -> Result<ast::LoadedFunction, ExecutionError> {
        assert_invariant!(
            self.linkage_view.has_linkage(package),
            "packages need to be linked before typing"
        );
        let package_address: AccountAddress = package.into();
        let original_address = self
            .linkage_view
            .original_package_id()?
            .unwrap_or(package_address);
        let module = to_identifier(module)?;
        let name = to_identifier(function)?;
        let storage_id = ModuleId::new(package_address, module.clone());
        let runtime_id = ModuleId::new(original_address, module);
        let mut data_store = SuiDataStore::new(&self.linkage_view, &[]);
        let signature = self
            .vm
            .get_runtime()
            .load_function(
                &runtime_id,
                name.as_ident_str(),
                &type_arguments,
                &mut data_store,
            )
            .map_err(|e| self.convert_vm_error(e))?;
        Ok(ast::LoadedFunction {
            storage_id,
            runtime_id,
            name,
            type_arguments,
            signature,
        })
    }

    pub fn load_type_input(&self, ty: TypeInput) -> Result<Type, ExecutionError> {
        let tag = ty
            .into_type_tag()
            .map_err(|e| make_invariant_violation!("{}", e.to_string()))?;
        self.load_type_tag(&tag)
    }

    pub fn load_type_tag(&self, tag: &TypeTag) -> Result<Type, ExecutionError> {
        load_type(self.vm, self.linkage_view, &[], tag).map_err(|e| self.convert_vm_error(e))
    }

    pub fn load_type_from_struct(&self, tag: &StructTag) -> Result<Type, ExecutionError> {
        load_type_from_struct(self.vm, self.linkage_view, &[], tag)
            .map_err(|e| self.convert_vm_error(e))
    }

    pub fn gas_coin_type(&self) -> Result<&Type, ExecutionError> {
        get_or_init_ty!(self, gas_coin_type, GasCoin::type_())
    }

    pub fn upgrade_ticket_type(&self) -> Result<&Type, ExecutionError> {
        get_or_init_ty!(self, upgrade_ticket_type, UpgradeTicket::type_())
    }

    pub fn upgrade_receipt_type(&self) -> Result<&Type, ExecutionError> {
        get_or_init_ty!(self, upgrade_receipt_type, UpgradeReceipt::type_())
    }

    pub fn abilities(&self, ty: &Type) -> Result<AbilitySet, ExecutionError> {
        self.vm
            .get_runtime()
            .get_type_abilities(ty)
            .map_err(|e| self.convert_vm_error(e))
    }

    pub fn datatype(&self, tag: CachedTypeIndex) -> Result<Arc<CachedDatatype>, ExecutionError> {
        match self.vm.get_runtime().get_type(tag) {
            Some(ty) => Ok(ty),
            None => invariant_violation!("Cannot retreive loaded type: {:?}", tag),
        }
    }

    pub fn read_object(&self, id: &ObjectID) -> Result<&Object, ExecutionError> {
        let Some(obj) = self.state_view.read_object(id) else {
            // protected by transaction input checker
            invariant_violation!("Object {:?} does not exist", id);
        };
        Ok(obj)
    }
}

fn to_identifier(name: String) -> Result<Identifier, ExecutionError> {
    Identifier::new(name).map_err(|e| {
        ExecutionError::new_with_source(ExecutionErrorKind::VMInvariantViolation, e.to_string())
    })
}
