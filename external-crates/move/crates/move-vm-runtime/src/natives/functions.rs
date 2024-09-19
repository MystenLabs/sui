// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::type_cache::TypeCache, natives::extensions::NativeContextExtensions,
    vm::interpreter::state::MachineState,
};
use move_binary_format::errors::{PartialVMError, PartialVMResult};
use move_core_types::{
    account_address::AccountAddress,
    annotated_value as A,
    gas_algebra::InternalGas,
    identifier::Identifier,
    language_storage::TypeTag,
    runtime_value as R,
    vm_status::{StatusCode, StatusType},
};
use move_vm_config::runtime::VMRuntimeLimitsConfig;
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, values::Value,
};
use parking_lot::RwLock;
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

    pub fn empty_for_testing() -> PartialVMResult<Self> {
        let map = HashMap::new();
        Ok(Self(map))
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

pub struct NativeContext<'a, 'b, 'c> {
    // If this native was the base invocation, we do not create a machine state. This is only used
    // for printing stack traces, and in that case we will print that there is no call stack.
    state: Option<&'a MachineState>,
    type_cache: &'a RwLock<TypeCache>,
    extensions: &'a mut NativeContextExtensions<'b>,
    runtime_limits_config: &'c VMRuntimeLimitsConfig,
    gas_left: RefCell<InternalGas>,
    gas_budget: InternalGas,
}

impl<'a, 'b, 'c> NativeContext<'a, 'b, 'c> {
    pub(crate) fn new(
        state: Option<&'a MachineState>,
        type_cache: &'a RwLock<TypeCache>,
        extensions: &'a mut NativeContextExtensions<'b>,
        runtime_limits_config: &'c VMRuntimeLimitsConfig,
        gas_budget: InternalGas,
    ) -> Self {
        Self {
            state,
            type_cache,
            extensions,
            runtime_limits_config,
            gas_left: RefCell::new(gas_budget),
            gas_budget,
        }
    }

    /// Limits imposed at runtime
    pub fn runtime_limits_config(&self) -> &VMRuntimeLimitsConfig {
        self.runtime_limits_config
    }
}

macro_rules! debug_writeln {
    ($($toks: tt)*) => {
        writeln!($($toks)*).map_err(|_|
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("failed to write to buffer".to_string())
        )
    };
}

impl<'a, 'b, 'c> NativeContext<'a, 'b, 'c> {
    pub fn type_to_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.type_cache.write().type_to_type_tag(ty)
    }

    pub fn type_to_runtime_type_tag(&self, ty: &Type) -> PartialVMResult<TypeTag> {
        self.type_cache.write().type_to_runtime_type_tag(ty)
    }

    pub fn type_to_type_layout(&self, ty: &Type) -> PartialVMResult<Option<R::MoveTypeLayout>> {
        match self.type_cache.write().type_to_type_layout(ty) {
            Ok(ty_layout) => Ok(Some(ty_layout)),
            Err(e) if e.major_status().status_type() == StatusType::InvariantViolation => Err(e),
            Err(_) => Ok(None),
        }
    }

    pub fn type_to_fully_annotated_layout(
        &self,
        ty: &Type,
    ) -> PartialVMResult<Option<A::MoveTypeLayout>> {
        match self.type_cache.write().type_to_fully_annotated_layout(ty) {
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

    pub fn print_stack_trace<B: Write>(&self, buf: &mut B) -> PartialVMResult<()> {
        // If this native was a base invocation, it won't have a stack to speak of.
        if let Some(state) = self.state {
            state.debug_print_stack_trace(buf, self.type_cache)
        } else {
            debug_writeln!(buf, "No Call Stack Available")?;
            debug_writeln!(buf, "Base Native Invocations Do Not Create Call Stacks")
        }
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

// Manual implementation of Debug for NativeFunctions
impl std::fmt::Debug for NativeFunctions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeFunctions")
            .field("functions", &format_function_map(&self.0))
            .finish()
    }
}

// Helper function to format the HashMap structure
fn format_function_map(
    map: &HashMap<AccountAddress, HashMap<String, HashMap<String, NativeFunction>>>,
) -> String {
    let mut result = String::new();

    for (address, module_map) in map {
        result.push_str(&format!("Account: {:?}\n", address));
        for (module_name, function_map) in module_map {
            result.push_str(&format!("  Module: {}\n", module_name));
            for (function_name, _) in function_map {
                result.push_str(&format!("    Function: {}\n", function_name));
            }
        }
    }

    result
}
