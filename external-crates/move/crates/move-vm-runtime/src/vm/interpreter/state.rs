// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::{
        arena::ArenaPointer,
        type_cache::{self, TypeCache},
    },
    jit::runtime::ast::{CallType, Function, Module},
    shared::constants::{
        CALL_STACK_SIZE_LIMIT, MAX_TYPE_INSTANTIATION_NODES, OPERAND_STACK_SIZE_LIMIT,
    },
    vm::runtime_vtables::RuntimeVTables,
};
use move_binary_format::{
    errors::*,
    file_format::{
        Constant, ConstantPoolIndex, FieldHandleIndex, FieldInstantiationIndex,
        FunctionInstantiationIndex, SignatureIndex, StructDefInstantiationIndex,
        StructDefinitionIndex, VariantHandleIndex, VariantInstantiationHandleIndex, VariantTag,
    },
    CompiledModule,
};
use move_core_types::{
    language_storage::{ModuleId, TypeTag},
    vm_status::{StatusCode, StatusType},
};
use move_vm_types::{
    loaded_data::runtime_types::{CachedTypeIndex, Type},
    values::{self, Locals, VMValueCast, Value},
    views::TypeView,
};
use parking_lot::RwLock;

use std::{cmp::min, fmt::Write, sync::Arc};
use tracing::error;

macro_rules! debug_write {
    ($($toks: tt)*) => {
        write!($($toks)*).map_err(|_|
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("failed to write to buffer".to_string())
        )
    };
}

macro_rules! debug_writeln {
    ($($toks: tt)*) => {
        writeln!($($toks)*).map_err(|_|
            PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR)
                .with_message("failed to write to buffer".to_string())
        )
    };
}

macro_rules! set_err_info {
    ($frame:ident, $e:expr) => {{
        let function = $frame.function();
        $e.at_code_offset(function.index(), $frame.pc)
            .finish($frame.location())
    }};
}

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

/// `MachineState` instances can execute Move functions.
///
/// An `MachineState` instance is a stand alone execution context for a function.
/// It mimics execution on a single thread, with an call stack and an operand stack.
pub(crate) struct MachineState {
    /// Operand stack, where Move `Value`s are stored for stack operations.
    pub(super) operand_stack: Stack,
    /// The stack of active functions.
    pub(super) call_stack: CallStack,
    /// The current frame we are computing in.
    pub(super) current_frame: CallFrame,
}

/// The operand stack.
pub(super) struct Stack {
    pub(super) value: Vec<Value>,
}

/// A call stack.
// #[derive(Debug)]
pub(super) struct CallStack(Vec<CallFrame>);

// A Resolver is a simple and small structure allocated on the stack and used by the
// interpreter. It's the only API known to the interpreter and it's tailored to the interpreter
// needs.
#[derive(Debug)]
pub(crate) struct ModuleDefinitionResolver {
    compiled: Arc<CompiledModule>,
    loaded: Arc<Module>,
}

/// A `Frame` is the execution context for a function. It holds the locals of the function and
/// the function itself.
#[derive(Debug)]
pub(super) struct CallFrame {
    pub(super) function: ArenaPointer<Function>,
    pub(super) pc: u16,
    pub(super) resolver: ModuleDefinitionResolver,
    pub(super) locals: Locals,
    pub(super) ty_args: Vec<Type>,
}

pub(super) struct TypeWithTypeCache<'a, 'b> {
    pub(super) ty: &'a Type,
    pub(super) type_cache: &'b RwLock<TypeCache>,
}

// -------------------------------------------------------------------------------------------------
// impl Blocks
// -------------------------------------------------------------------------------------------------

impl MachineState {
    pub(super) fn new(initial_frame: CallFrame) -> Self {
        MachineState {
            operand_stack: Stack::new(),
            call_stack: CallStack::new(),
            current_frame: initial_frame,
        }
    }

    /// Push a `Value` on the stack if the max stack size has not been reached. Abort execution
    /// otherwise.
    #[inline]
    pub fn push_operand(&mut self, value: Value) -> PartialVMResult<()> {
        self.operand_stack.push(value)
    }

    /// Pop a `Value` off the stack or abort execution if the stack is empty.
    #[inline]
    pub fn pop_operand(&mut self) -> PartialVMResult<Value> {
        self.operand_stack.pop()
    }

    /// Pop a `Value` of a given type off the stack. Abort if the value is not of the given
    /// type or if the stack is empty.
    #[inline]
    pub fn pop_operand_as<T>(&mut self) -> PartialVMResult<T>
    where
        Value: VMValueCast<T>,
    {
        self.operand_stack.pop_as()
    }

    /// Pop n values off the stack.
    #[inline]
    pub fn pop_n_operands(&mut self, n: u16) -> PartialVMResult<Vec<Value>> {
        self.operand_stack.pop_n(n)
    }

    #[inline]
    pub fn last_n_operands(
        &self,
        n: usize,
    ) -> PartialVMResult<impl ExactSizeIterator<Item = &Value>> {
        self.operand_stack.last_n(n)
    }

    /// Push a new call frame (setting the machine's `current_frame` to the provided `new_frame`).
    /// Produces a `VMError` using the machine state's previous `current_frame` if this would
    /// overflow the call stack.
    pub(super) fn push_call_frame(&mut self, new_frame: CallFrame) -> VMResult<()> {
        let prev_frame = std::mem::replace(&mut self.current_frame, new_frame);
        // cswords: This code previously took the "prev frame" as the one to push, so the error here
        // is logically the same despite the change.
        self.call_stack.push(prev_frame).map_err(|frame| {
            let err = PartialVMError::new(StatusCode::CALL_STACK_OVERFLOW);
            let err = set_err_info!(frame, err);
            self.maybe_core_dump_with_frame(err, &frame)
        })
    }

    pub(super) fn pop_call_frame(&mut self) -> Option<CallFrame> {
        self.call_stack.pop()
    }

    //
    // Debugging and logging helpers.
    //

    /// Given an `VMStatus` generate a core dump if the error is an `InvariantViolation`. Uses the
    /// `current_frame` on the state to perform the core dump.
    pub fn maybe_core_dump(&self, err: VMError) -> VMError {
        // a verification error cannot happen at runtime so change it into an invariant violation.
        let err = if err.status_type() == StatusType::Verification {
            error!("Verification error during runtime: {:?}", err);
            let new_err = PartialVMError::new(StatusCode::VERIFICATION_ERROR);
            let new_err = match err.message() {
                None => new_err,
                Some(msg) => new_err.with_message(msg.to_owned()),
            };
            new_err.finish(err.location().clone())
        } else {
            err
        };
        self.maybe_core_dump_with_frame(err, &self.current_frame)
    }

    /// Given an `VMStatus` generate a core dump if the error is an `InvariantViolation`. Uses the
    /// provided `CallFrame` to perform the core dump.
    fn maybe_core_dump_with_frame(&self, err: VMError, frame: &CallFrame) -> VMError {
        if err.status_type() == StatusType::InvariantViolation {
            let state = self.internal_state_str(frame);
            error!(
                "Error: {:?}\nCORE DUMP: >>>>>>>>>>>>\n{}\n<<<<<<<<<<<<\n",
                err, state,
            );
        }
        err
    }

    #[allow(dead_code)]
    pub(super) fn debug_print_frame<B: Write>(
        &self,
        buf: &mut B,
        type_cache: &RwLock<TypeCache>,
        idx: usize,
        frame: &CallFrame,
    ) -> PartialVMResult<()> {
        // Print out the function name with type arguments.
        let _func_ptr = frame.function;
        let func = frame.function();

        debug_write!(buf, "    [{}] ", idx)?;
        let module = func.module_id();
        debug_write!(buf, "{}::{}::", module.address(), module.name(),)?;

        debug_write!(buf, "{}", func.name())?;
        let ty_args = frame.ty_args();
        let mut ty_tags = vec![];
        for ty in ty_args {
            ty_tags.push(type_cache.write().type_to_type_tag(ty)?);
        }
        if !ty_tags.is_empty() {
            debug_write!(buf, "<")?;
            let mut it = ty_tags.iter();
            if let Some(tag) = it.next() {
                debug_write!(buf, "{}", tag)?;
                for tag in it {
                    debug_write!(buf, ", ")?;
                    debug_write!(buf, "{}", tag)?;
                }
            }
            debug_write!(buf, ">")?;
        }
        debug_writeln!(buf)?;

        // Print out the current instruction.
        debug_writeln!(buf)?;
        debug_writeln!(buf, "        Code:")?;
        let pc = frame.pc as usize;
        let code = func.code();
        let before = if pc > 3 { pc - 3 } else { 0 };
        let after = min(code.len(), pc + 4);
        for (idx, instr) in code.iter().enumerate().take(pc).skip(before) {
            debug_writeln!(buf, "            [{}] {:?}", idx, instr)?;
        }
        debug_writeln!(buf, "          > [{}] {:?}", pc, &code[pc])?;
        for (idx, instr) in code.iter().enumerate().take(after).skip(pc + 1) {
            debug_writeln!(buf, "            [{}] {:?}", idx, instr)?;
        }

        // Print out the locals.
        debug_writeln!(buf)?;
        debug_writeln!(buf, "        Locals:")?;
        if func.local_count() > 0 {
            values::debug::print_locals(buf, &frame.locals)?;
            debug_writeln!(buf)?;
        } else {
            debug_writeln!(buf, "            (none)")?;
        }

        debug_writeln!(buf)?;
        Ok(())
    }

    #[allow(dead_code)]
    pub(crate) fn debug_print_stack_trace<B: Write>(
        &self,
        buf: &mut B,
        type_cache: &RwLock<TypeCache>,
    ) -> PartialVMResult<()> {
        debug_writeln!(buf, "Call Stack:")?;
        self.debug_print_frame(buf, type_cache, 0, &self.current_frame)?;
        for (i, frame) in self.call_stack.0.iter().enumerate() {
            self.debug_print_frame(buf, type_cache, i + 1, frame)?;
        }
        debug_writeln!(buf, "Operand Stack:")?;
        for (idx, val) in self.operand_stack.value.iter().enumerate() {
            // TODO: Currently we do not know the types of the values on the operand stack.
            // Revisit.
            debug_write!(buf, "    [{}] ", idx)?;
            values::debug::print_value(buf, val)?;
            debug_writeln!(buf)?;
        }
        Ok(())
    }

    /// Generate a string which is the status of the interpreter: call stack, current bytecode
    /// stream, locals and operand stack.
    ///
    /// It is used when generating a core dump but can be used for debugging of the interpreter.
    /// It will be exposed via a debug module to give developers a way to print the internals
    /// of an execution.
    fn internal_state_str(&self, current_frame: &CallFrame) -> String {
        let mut internal_state = "Call stack:\n".to_string();

        for (i, frame) in self.call_stack.0.iter().enumerate() {
            internal_state.push_str(
                format!(
                    " frame #{}: {} [pc = {}]\n",
                    i,
                    frame.function().pretty_string(),
                    frame.pc,
                )
                .as_str(),
            );
        }
        internal_state.push_str(
            format!(
                "*frame #{}: {} [pc = {}]:\n",
                self.call_stack.0.len(),
                current_frame.function().pretty_string(),
                current_frame.pc,
            )
            .as_str(),
        );
        let code = current_frame.function().code();
        let pc = current_frame.pc as usize;
        if pc < code.len() {
            let mut i = 0;
            for bytecode in &code[..pc] {
                internal_state.push_str(format!("{}> {:?}\n", i, bytecode).as_str());
                i += 1;
            }
            internal_state.push_str(format!("{}* {:?}\n", i, code[pc]).as_str());
        }
        internal_state.push_str(
            format!(
                "Locals ({:x}):\n{}\n",
                current_frame.locals.raw_address(),
                current_frame.locals
            )
            .as_str(),
        );
        internal_state.push_str("Operand Stack:\n");
        for value in &self.operand_stack.value {
            internal_state.push_str(format!("{}\n", value).as_str());
        }
        internal_state
    }

    pub(super) fn set_location(&self, err: PartialVMError) -> VMError {
        err.finish(self.current_frame.location())
    }

    pub(super) fn get_internal_state(&self) -> ExecutionState {
        self.get_stack_frames(usize::MAX)
    }

    /// Get count stack frames starting from the top of the stack.
    pub fn get_stack_frames(&self, count: usize) -> ExecutionState {
        // collect frames in the reverse order as this is what is
        // normally expected from the stack trace (outermost frame
        // is the last one)
        let stack_trace = self
            .call_stack
            .0
            .iter()
            .rev()
            .take(count)
            .map(|frame| {
                let fun = frame.function();
                (fun.module_id().clone(), fun.index(), frame.pc)
            })
            .collect();
        ExecutionState::new(stack_trace)
    }
}

impl Stack {
    /// Create a new empty operand stack.
    fn new() -> Self {
        Stack { value: vec![] }
    }

    /// Push a `Value` on the stack if the max stack size has not been reached. Abort execution
    /// otherwise.
    fn push(&mut self, value: Value) -> PartialVMResult<()> {
        if self.value.len() < OPERAND_STACK_SIZE_LIMIT {
            self.value.push(value);
            Ok(())
        } else {
            Err(PartialVMError::new(StatusCode::EXECUTION_STACK_OVERFLOW))
        }
    }

    /// Pop a `Value` off the stack or abort execution if the stack is empty.
    fn pop(&mut self) -> PartialVMResult<Value> {
        self.value
            .pop()
            .ok_or_else(|| PartialVMError::new(StatusCode::EMPTY_VALUE_STACK))
    }

    /// Pop a `Value` of a given type off the stack. Abort if the value is not of the given
    /// type or if the stack is empty.
    fn pop_as<T>(&mut self) -> PartialVMResult<T>
    where
        Value: VMValueCast<T>,
    {
        self.pop()?.value_as()
    }

    /// Pop n values off the stack.
    fn pop_n(&mut self, n: u16) -> PartialVMResult<Vec<Value>> {
        let remaining_stack_size = self
            .value
            .len()
            .checked_sub(n as usize)
            .ok_or_else(|| PartialVMError::new(StatusCode::EMPTY_VALUE_STACK))?;
        let args = self.value.split_off(remaining_stack_size);
        Ok(args)
    }

    fn last_n(&self, n: usize) -> PartialVMResult<impl ExactSizeIterator<Item = &Value>> {
        if self.value.len() < n {
            return Err(PartialVMError::new(StatusCode::EMPTY_VALUE_STACK)
                .with_message("Failed to get last n arguments on the argument stack".to_string()));
        }
        Ok(self.value[(self.value.len() - n)..].iter())
    }
}

impl CallStack {
    /// Create a new empty call stack.
    fn new() -> Self {
        CallStack(vec![])
    }

    /// Push a `Frame` on the call stack.
    fn push(&mut self, frame: CallFrame) -> ::std::result::Result<(), CallFrame> {
        if self.0.len() < CALL_STACK_SIZE_LIMIT {
            self.0.push(frame);
            Ok(())
        } else {
            Err(frame)
        }
    }

    /// Pop a `Frame` off the call stack.
    fn pop(&mut self) -> Option<CallFrame> {
        self.0.pop()
    }
}

impl CallFrame {
    /// Create a new `Frame` given a `Function` and the function's `ty_args` and `args`.
    /// This loads the locals, padding appropriately.
    #[inline]
    pub fn new(
        resolver: ModuleDefinitionResolver,
        function: ArenaPointer<Function>,
        ty_args: Vec<Type>,
        args: Vec<Value>,
    ) -> Self {
        let fun_ref = function.to_ref();
        let locals = Locals::new_from(args, fun_ref.local_count());
        CallFrame {
            pc: 0,
            locals,
            resolver,
            function,
            ty_args,
        }
    }

    pub(super) fn function<'a>(&self) -> &'a Function {
        self.function.to_ref()
    }

    pub(super) fn ty_args(&self) -> &[Type] {
        &self.ty_args
    }

    pub(super) fn resolver(&self) -> &ModuleDefinitionResolver {
        &self.resolver
    }

    pub(super) fn location(&self) -> Location {
        Location::Module(self.function().module_id().clone())
    }
}

impl ModuleDefinitionResolver {
    //
    // Creation: From a set of Runtime VTables and a ModuleId.
    //

    pub fn new(vtables: &RuntimeVTables, module_id: &ModuleId) -> PartialVMResult<Self> {
        let compiled = vtables.resolve_compiled_module(module_id)?;
        let loaded = vtables.resolve_loaded_module(module_id)?;
        Ok(Self { compiled, loaded })
    }

    //
    // Constant resolution
    //

    pub(crate) fn constant_at(&self, idx: ConstantPoolIndex) -> &Constant {
        self.compiled.constant_at(idx)
    }

    //
    // Function resolution
    //

    pub(crate) fn function_from_instantiation(&self, idx: FunctionInstantiationIndex) -> &CallType {
        &self.loaded.function_instantiation_at(idx.0).handle
    }

    pub(crate) fn instantiate_generic_function(
        &self,
        idx: FunctionInstantiationIndex,
        type_params: &[Type],
    ) -> PartialVMResult<Vec<Type>> {
        let loaded_module = &*self.loaded;
        let func_inst = loaded_module.function_instantiation_at(idx.0);
        let instantiation: Vec<_> = loaded_module
            .instantiation_signature_at(func_inst.instantiation_idx)?
            .iter()
            .map(|ty| type_cache::subst(ty, type_params))
            .collect::<PartialVMResult<_>>()?;

        // Check if the function instantiation over all generics is larger
        // than MAX_TYPE_INSTANTIATION_NODES.
        let mut sum_nodes = 1u64;
        for ty in type_params.iter().chain(instantiation.iter()) {
            sum_nodes = sum_nodes.saturating_add(type_cache::count_type_nodes(ty));
            if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
            }
        }
        Ok(instantiation)
    }

    //
    // Type resolution
    //

    pub(crate) fn get_struct_type(&self, idx: StructDefinitionIndex) -> Type {
        let struct_def = self.loaded.struct_at(idx);
        Type::Datatype(struct_def)
    }

    pub(crate) fn get_enum_type(&self, vidx: VariantHandleIndex) -> Type {
        let variant_handle = self.loaded.variant_handle_at(vidx);
        let enum_def = self.loaded.enum_at(variant_handle.enum_def);
        Type::Datatype(enum_def)
    }

    pub(crate) fn instantiate_struct_type(
        &self,
        idx: StructDefInstantiationIndex,
        ty_args: &[Type],
    ) -> PartialVMResult<Type> {
        let loaded_module = &*self.loaded;
        let struct_inst = loaded_module.struct_instantiation_at(idx.0);
        let instantiation =
            loaded_module.instantiation_signature_at(struct_inst.instantiation_idx)?;
        self.instantiate_type_common(struct_inst.def, instantiation, ty_args)
    }

    pub(crate) fn instantiate_enum_type(
        &self,
        vidx: VariantInstantiationHandleIndex,
        ty_args: &[Type],
    ) -> PartialVMResult<Type> {
        let loaded_module = &*self.loaded;
        let handle = loaded_module.variant_instantiation_handle_at(vidx);
        let enum_inst = loaded_module.enum_instantiation_at(handle.enum_def);
        let instantiation =
            loaded_module.instantiation_signature_at(enum_inst.instantiation_idx)?;
        self.instantiate_type_common(enum_inst.def, instantiation, ty_args)
    }

    fn instantiate_type_common(
        &self,
        gt_idx: CachedTypeIndex,
        type_params: &[Type],
        ty_args: &[Type],
    ) -> PartialVMResult<Type> {
        // Before instantiating the type, count the # of nodes of all type arguments plus
        // existing type instantiation.
        // If that number is larger than MAX_TYPE_INSTANTIATION_NODES, refuse to construct this type.
        // This prevents constructing larger and larger types via datatype instantiation.
        let mut sum_nodes = 1u64;
        for ty in ty_args.iter().chain(type_params.iter()) {
            sum_nodes = sum_nodes.saturating_add(type_cache::count_type_nodes(ty));
            if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
            }
        }

        Ok(Type::DatatypeInstantiation(Box::new((
            gt_idx,
            type_params
                .iter()
                .map(|ty| type_cache::subst(ty, ty_args))
                .collect::<PartialVMResult<_>>()?,
        ))))
    }

    fn single_type_at(&self, idx: SignatureIndex) -> &Type {
        self.loaded.single_type_at(idx)
    }

    pub(crate) fn instantiate_single_type(
        &self,
        idx: SignatureIndex,
        ty_args: &[Type],
    ) -> PartialVMResult<Type> {
        let ty = self.single_type_at(idx);
        if !ty_args.is_empty() {
            type_cache::subst(ty, ty_args)
        } else {
            Ok(ty.clone())
        }
    }

    //
    // Fields resolution
    //

    pub(crate) fn field_offset(&self, idx: FieldHandleIndex) -> usize {
        self.loaded.field_offset(idx)
    }

    pub(crate) fn field_instantiation_offset(&self, idx: FieldInstantiationIndex) -> usize {
        self.loaded.field_instantiation_offset(idx)
    }

    pub(crate) fn field_count(&self, idx: StructDefinitionIndex) -> u16 {
        self.loaded.field_count(idx.0)
    }

    pub(crate) fn variant_field_count_and_tag(
        &self,
        vidx: VariantHandleIndex,
    ) -> (u16, VariantTag) {
        self.loaded.variant_field_count(vidx)
    }

    pub(crate) fn field_instantiation_count(&self, idx: StructDefInstantiationIndex) -> u16 {
        self.loaded.field_instantiation_count(idx.0)
    }

    pub(crate) fn variant_instantiantiation_field_count_and_tag(
        &self,
        vidx: VariantInstantiationHandleIndex,
    ) -> (u16, VariantTag) {
        self.loaded
            .variant_instantiantiation_field_count_and_tag(vidx)
    }
}

// -------------------------------------------------------------------------------------------------
// Other impls
// -------------------------------------------------------------------------------------------------

impl<'a, 'b> TypeView for TypeWithTypeCache<'a, 'b> {
    fn to_type_tag(&self) -> TypeTag {
        self.type_cache.write().type_to_type_tag(self.ty).unwrap()
    }
}
