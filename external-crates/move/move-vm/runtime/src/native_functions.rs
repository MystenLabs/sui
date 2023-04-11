// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    config::VMRuntimeLimitsConfig, interpreter::Interpreter, loader::Resolver,
    native_extensions::NativeContextExtensions,
};
use move_binary_format::errors::{ExecutionState, PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress,
    gas_algebra::InternalGas,
    identifier::Identifier,
    language_storage::TypeTag,
    value::MoveTypeLayout,
    vm_status::{StatusCode, StatusType},
};
use move_vm_types::{
    data_store::DataStore, loaded_data::runtime_types::Type, natives::function::NativeResult,
    values::Value,
};
use std::{
    cell::RefCell,
    collections::{HashMap, VecDeque},
    fmt::Write,
    sync::Arc,
};

pub type UnboxedNativeFunction = dyn Fn(&mut NativeContext, Vec<Type>, VecDeque<Value>) -> PartialVMResult<NativeResult>
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

pub(crate) struct NativeFunctions(
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

pub struct NativeContext<'a, 'b> {
    interpreter: &'a mut Interpreter,
    data_store: &'a mut dyn DataStore,
    resolver: &'a Resolver<'a>,
    extensions: &'a mut NativeContextExtensions<'b>,
    gas_left: RefCell<InternalGas>,
    gas_budget: InternalGas,
}

impl<'a, 'b> NativeContext<'a, 'b> {
    pub(crate) fn new(
        interpreter: &'a mut Interpreter,
        data_store: &'a mut dyn DataStore,
        resolver: &'a Resolver<'a>,
        extensions: &'a mut NativeContextExtensions<'b>,
        gas_budget: InternalGas,
    ) -> Self {
        Self {
            interpreter,
            data_store,
            resolver,
            extensions,
            gas_left: RefCell::new(gas_budget),
            gas_budget,
        }
    }

    /// Limits imposed at runtime
    pub fn runtime_limits_config(&self) -> &VMRuntimeLimitsConfig {
        self.interpreter.runtime_limits_config()
    }
}

impl<'a, 'b> NativeContext<'a, 'b> {
    pub fn print_stack_trace<B: Write>(&self, buf: &mut B) -> PartialVMResult<()> {
        self.interpreter
            .debug_print_stack_trace(buf, self.resolver.loader())
    }

    pub fn save_event(
        &mut self,
        guid: Vec<u8>,
        seq_num: u64,
        ty: Type,
        val: Value,
    ) -> PartialVMResult<bool> {
        match self.data_store.emit_event(guid, seq_num, ty, val) {
            Ok(()) => Ok(true),
            Err(e) if e.major_status().status_type() == StatusType::InvariantViolation => Err(e),
            Err(_) => Ok(false),
        }
    }

    pub fn events(&self) -> &Vec<(Vec<u8>, u64, Type, MoveTypeLayout, Value)> {
        self.data_store.events()
    }

    pub fn type_to_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.resolver.loader().type_to_type_tag(ty)
    }

    pub fn type_to_type_layout(&self, ty: &Type) -> PartialVMResult<Option<MoveTypeLayout>> {
        match self.resolver.type_to_type_layout(ty) {
            Ok(ty_layout) => Ok(Some(ty_layout)),
            Err(e) if e.major_status().status_type() == StatusType::InvariantViolation => Err(e),
            Err(_) => Ok(None),
        }
    }

    pub fn type_to_fully_annotated_layout(
        &self,
        ty: &Type,
    ) -> PartialVMResult<Option<MoveTypeLayout>> {
        match self.resolver.type_to_fully_annotated_layout(ty) {
            Ok(ty_layout) => Ok(Some(ty_layout)),
            Err(e) if e.major_status().status_type() == StatusType::InvariantViolation => Err(e),
            Err(_) => Ok(None),
        }
    }

    pub fn extensions(&self) -> &NativeContextExtensions<'b> {
        self.extensions
    }

    pub fn extensions_mut(&mut self) -> &mut NativeContextExtensions<'b> {
        self.extensions
    }

    /// Get count stack frames, including the one of the called native function. This
    /// allows a native function to reflect about its caller.
    pub fn stack_frames(&self, count: usize) -> ExecutionState {
        self.interpreter.get_stack_frames(count)
    }

    pub fn charge_gas(&self, amount: InternalGas) -> bool {
        let mut gas_left = self.gas_left.borrow_mut();

        match gas_left.checked_sub(amount) {
            Some(x) => {
                *gas_left = x;
                true
            }
            None => false,
        }
    }

    pub fn gas_budget(&self) -> InternalGas {
        self.gas_budget
    }

    pub fn gas_used(&self) -> InternalGas {
        self.gas_budget.saturating_sub(*self.gas_left.borrow())
    }
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
