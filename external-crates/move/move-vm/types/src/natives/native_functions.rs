// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    loaded_data::runtime_types::Type,
    natives::{function::NativeResult, native_extensions::NativeContextExtensions},
    values::Value,
};
use move_binary_format::errors::{ExecutionState, PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress, gas_algebra::InternalGas, identifier::Identifier,
    language_storage::TypeTag, value::MoveTypeLayout, vm_status::StatusCode,
};
use move_vm_config::runtime::VMRuntimeLimitsConfig;
use std::{
    collections::{HashMap, VecDeque},
    fmt::Write,
    sync::Arc,
};

pub type UnboxedNativeFunction = dyn Fn(&mut dyn NativeContext, Vec<Type>, VecDeque<Value>) -> PartialVMResult<NativeResult>
    + Send
    + Sync
    + 'static;

pub type NativeFunction = Arc<UnboxedNativeFunction>;

pub type NativeFunctionTable = Vec<(AccountAddress, Identifier, Identifier, NativeFunction)>;

pub fn make_table(
    addr: AccountAddress,
    elems: &[(&str, &str, NativeFunction)],
) -> NativeFunctionTable {
    make_table_from_iter(addr, elems.iter().cloned())
}

pub fn make_table_from_iter<S: Into<Box<str>>>(
    addr: AccountAddress,
    elems: impl IntoIterator<Item = (S, S, NativeFunction)>,
) -> NativeFunctionTable {
    elems
        .into_iter()
        .map(|(module_name, func_name, func)| {
            (
                addr,
                Identifier::new(module_name).unwrap(),
                Identifier::new(func_name).unwrap(),
                func,
            )
        })
        .collect()
}

pub struct NativeFunctions(
    HashMap<AccountAddress, HashMap<String, HashMap<String, NativeFunction>>>,
);

impl NativeFunctions {
    pub fn resolve(
        &self,
        addr: &AccountAddress,
        module_name: &str,
        func_name: &str,
    ) -> Option<NativeFunction> {
        self.0.get(addr)?.get(module_name)?.get(func_name).cloned()
    }

    pub fn new<I>(natives: I) -> PartialVMResult<Self>
    where
        I: IntoIterator<Item = (AccountAddress, Identifier, Identifier, NativeFunction)>,
    {
        let mut map = HashMap::new();
        for (addr, module_name, func_name, func) in natives.into_iter() {
            let modules = map.entry(addr).or_insert_with(HashMap::new);
            let funcs = modules
                .entry(module_name.into_string())
                .or_insert_with(HashMap::new);

            if funcs.insert(func_name.into_string(), func).is_some() {
                return Err(PartialVMError::new(StatusCode::DUPLICATE_NATIVE_FUNCTION));
            }
        }
        Ok(Self(map))
    }
}

pub trait NativeContext<'b> {
    /// Limits imposed at runtime
    fn runtime_limits_config(&self) -> &VMRuntimeLimitsConfig;

    fn print_stack_trace(&self, buf: &mut dyn Write) -> PartialVMResult<()>;

    fn save_event(
        &mut self,
        guid: Vec<u8>,
        seq_num: u64,
        ty: Type,
        val: Value,
    ) -> PartialVMResult<bool>;

    fn events(&self) -> &Vec<(Vec<u8>, u64, Type, MoveTypeLayout, Value)>;

    fn type_to_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag>;

    fn type_to_runtime_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag>;

    fn type_to_type_layout(&self, ty: &Type) -> PartialVMResult<Option<MoveTypeLayout>>;

    fn type_to_fully_annotated_layout(&self, ty: &Type) -> PartialVMResult<Option<MoveTypeLayout>>;

    fn extensions(&self) -> &NativeContextExtensions<'b>;

    fn extensions_mut(&mut self) -> &mut NativeContextExtensions<'b>;

    /// Get count stack frames, including the one of the called native function. This
    /// allows a native function to reflect about its caller.
    fn stack_frames(&self, count: usize) -> ExecutionState;

    fn charge_gas(&self, amount: InternalGas) -> bool;

    fn gas_budget(&self) -> InternalGas;

    fn gas_used(&self) -> InternalGas;
}

/// Charge gas during a native call. If the charging fails, return early
#[macro_export]
macro_rules! native_charge_gas_early_exit {
    ($native_context:ident, $cost:expr) => {{
        use move_core_types::vm_status::sub_status::NFE_OUT_OF_GAS;
        if !$native_context.charge_gas($cost) {
            // Exhausted all in budget. terminate early
            return Ok(NativeResult::err(
                $native_context.gas_budget(),
                NFE_OUT_OF_GAS,
            ));
        }
    }};
}

/// Total cost of a native call so far
#[macro_export]
macro_rules! native_gas_total_cost {
    ($native_context:ident, $gas_left:ident) => {{
        // Its okay to unwrap because the budget can never be less than the gas left
        $native_context.gas_budget().checked_sub($gas_left).unwrap()
    }};
}
