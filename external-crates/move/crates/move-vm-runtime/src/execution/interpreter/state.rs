// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    execution::{
        dispatch_tables::{count_type_nodes, subst, VMDispatchTables, VirtualTableKey},
        interpreter::{locals::MachineHeap, set_err_info},
        values::values_impl::{self as values, VMValueCast, Value},
    },
    jit::execution::ast::{CallType, Constant, Function, Module, Type},
    shared::{
        constants::{
            CALL_STACK_SIZE_LIMIT, MAX_TYPE_INSTANTIATION_NODES, OPERAND_STACK_SIZE_LIMIT,
        },
        views::TypeView,
        vm_pointer::VMPointer,
    },
};
use move_binary_format::{
    errors::*,
    file_format::{
        ConstantPoolIndex, FieldHandleIndex, FieldInstantiationIndex, FunctionInstantiationIndex,
        SignatureIndex, StructDefInstantiationIndex, StructDefinitionIndex, VariantHandleIndex,
        VariantInstantiationHandleIndex, VariantTag,
    },
};
use move_core_types::{
    language_storage::{ModuleId, TypeTag},
    vm_status::{StatusCode, StatusType},
};

use std::{cmp::min, fmt::Write, sync::Arc};
use tracing::error;

use super::locals::StackFrame;

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

// -------------------------------------------------------------------------------------------------
// Types
// -------------------------------------------------------------------------------------------------

/// `MachineState` instances can execute Move functions.
///
/// An `MachineState` instance is a stand alone execution context for a function.
/// It mimics execution on a single thread, with an call stack and an operand stack.
pub(crate) struct MachineState {
    pub(crate) call_stack: CallStack,
    /// Operand stack, where Move `Value`s are stored for stack operations.
    pub(crate) operand_stack: ValueStack,
}

/// The operand stack.
pub(crate) struct ValueStack {
    pub(crate) value: Vec<Value>,
}

/// A call stack.
// #[derive(Debug)]
pub(crate) struct CallStack {
    /// The current frame we are computing in.
    pub(crate) current_frame: CallFrame,
    /// The current heap.
    pub(crate) heap: MachineHeap,
    /// The stack of active functions.
    pub(crate) frames: Vec<CallFrame>,
}

// A Resolver is a simple and small structure allocated on the stack and used by the
// interpreter. It's the only API known to the interpreter and it's tailored to the interpreter
// needs.
#[derive(Debug)]
pub(crate) struct ModuleDefinitionResolver {
    module: Arc<Module>,
}

/// A `Frame` is the execution context for a function. It holds the locals of the function and
/// the function itself.
#[derive(Debug)]
pub(crate) struct CallFrame {
    pub(crate) pc: u16,
    pub(crate) function: VMPointer<Function>,
    pub(crate) resolver: ModuleDefinitionResolver,
    pub(crate) stack_frame: StackFrame,
    pub(crate) ty_args: Vec<Type>,
}

pub(super) struct ResolvableType<'a, 'b> {
    pub(super) ty: &'a Type,
    pub(super) vtables: &'b VMDispatchTables,
}

// -------------------------------------------------------------------------------------------------
// impl Blocks
// -------------------------------------------------------------------------------------------------

impl MachineState {
    pub(super) fn new(call_stack: CallStack) -> Self {
        MachineState {
            operand_stack: ValueStack::new(),
            call_stack,
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
    #[inline]
    pub fn push_call(
        &mut self,
        resolver: ModuleDefinitionResolver,
        function: VMPointer<Function>,
        ty_args: Vec<Type>,
        args: Vec<Value>,
    ) -> VMResult<()> {
        self.call_stack.push_call(resolver, function, ty_args, args)
    }

    /// Returns true if there is a frame to pop.
    #[inline]
    pub(super) fn can_pop_call_frame(&self) -> bool {
        !self.call_stack.frames.is_empty()
    }

    /// Frees the current stack frame and puts the previous one there, or throws an error if there
    /// is not a frame to pop.
    #[inline]
    pub(super) fn pop_call_frame(&mut self) -> VMResult<()> {
        self.call_stack.pop_frame()
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
                None => new_err.with_message("No message provided for core dump".to_owned()),
                Some(msg) => new_err.with_message(msg.to_owned()),
            };
            new_err.finish(err.location().clone())
        } else {
            err
        };
        if err.status_type() == StatusType::InvariantViolation {
            let state = self.internal_state_str();
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
        vtables: &VMDispatchTables,
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
            ty_tags.push(vtables.type_to_type_tag(ty)?);
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
            values::debug::print_stack_frame(buf, &frame.stack_frame)?;
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
        vtables: &VMDispatchTables,
    ) -> PartialVMResult<()> {
        debug_writeln!(buf, "Call Stack:")?;
        self.debug_print_frame(buf, vtables, 0, &self.call_stack.current_frame)?;
        for (i, frame) in self.call_stack.frames.iter().enumerate() {
            self.debug_print_frame(buf, vtables, i + 1, frame)?;
        }
        debug_writeln!(buf, "Operand Stack:")?;
        for (idx, val) in self.operand_stack.value.iter().enumerate() {
            // [FUTURE] Currently we do not know the types of the values on the operand stack.
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
    fn internal_state_str(&self) -> String {
        let mut internal_state = "Call stack:\n".to_string();

        for (i, frame) in self.call_stack.frames.iter().enumerate() {
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
                self.call_stack.frames.len(),
                self.call_stack.current_frame.function().pretty_string(),
                self.call_stack.current_frame.pc,
            )
            .as_str(),
        );
        let code = self.call_stack.current_frame.function().code();
        let pc = self.call_stack.current_frame.pc as usize;
        if pc < code.len() {
            let mut i = 0;
            for bytecode in &code[..pc] {
                internal_state.push_str(format!("{}> {:?}\n", i, bytecode).as_str());
                i += 1;
            }
            internal_state.push_str(format!("{}* {:?}\n", i, code[pc]).as_str());
        }
        internal_state
            .push_str(format!("Locals:\n{}\n", self.call_stack.current_frame.stack_frame).as_str());
        internal_state.push_str("Operand Stack:\n");
        for value in &self.operand_stack.value {
            internal_state.push_str(format!("{}\n", value).as_str());
        }
        internal_state
    }

    pub(super) fn set_location(&self, err: PartialVMError) -> VMError {
        err.finish(self.call_stack.current_frame.location())
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
            .frames
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

impl ValueStack {
    /// Create a new empty operand stack.
    fn new() -> Self {
        ValueStack { value: vec![] }
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
        VMValueCast::cast(self.pop()?)
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

    pub(crate) fn len(&self) -> usize {
        self.value.len()
    }

    pub(crate) fn value_at(&self, n: usize) -> Option<&Value> {
        self.value.get(n)
    }
}

impl CallStack {
    /// Create a new empty call stack.
    pub fn new(
        resolver: ModuleDefinitionResolver,
        function: VMPointer<Function>,
        ty_args: Vec<Type>,
        args: Vec<Value>,
    ) -> PartialVMResult<Self> {
        let mut heap = MachineHeap::new();

        let fun_ref = function.to_ref();
        let stack_frame = heap.allocate_stack_frame(args, fun_ref.local_count())?;
        let current_frame = CallFrame {
            pc: 0,
            stack_frame,
            resolver,
            function,
            ty_args,
        };

        Ok(Self {
            current_frame,
            heap,
            frames: vec![],
        })
    }

    /// Create a new `Frame` given a `Function` and the function's `ty_args` and `args`.
    /// This loads the locals, padding appropriately, and sets the call stack's current frame.
    #[inline]
    pub fn push_call(
        &mut self,
        resolver: ModuleDefinitionResolver,
        function: VMPointer<Function>,
        ty_args: Vec<Type>,
        args: Vec<Value>,
    ) -> VMResult<()> {
        let fun_ref = function.to_ref();
        let stack_frame = self
            .heap
            .allocate_stack_frame(args, fun_ref.local_count())
            .map_err(|err| set_err_info!(&self.current_frame, err))?;
        let new_frame = CallFrame {
            pc: 0,
            stack_frame,
            resolver,
            function,
            ty_args,
        };
        if self.frames.len() < CALL_STACK_SIZE_LIMIT {
            let prev_frame = std::mem::replace(&mut self.current_frame, new_frame);
            self.frames.push(prev_frame);
            Ok(())
        } else {
            let err = PartialVMError::new(StatusCode::CALL_STACK_OVERFLOW);
            let err = set_err_info!(new_frame, err);
            Err(err)
        }
    }

    /// Pop a `Frame` off the call stack, freeing the old one. Returns an error if there is no
    /// frame to pop.
    #[inline]
    fn pop_frame(&mut self) -> VMResult<()> {
        let Some(return_frame) = self.frames.pop() else {
            let err = PartialVMError::new(StatusCode::UNKNOWN_INVARIANT_VIOLATION_ERROR);
            let err = set_err_info!(self.current_frame, err);
            return Err(err);
        };
        let frame = std::mem::replace(&mut self.current_frame, return_frame);
        let index = frame.function().index();
        let pc = frame.pc;
        let loc = frame.location();
        self.heap
            .free_stack_frame(frame.stack_frame)
            .map_err(|e| e.at_code_offset(index, pc).finish(loc))
    }
}

impl CallFrame {
    pub(super) fn function<'a>(&self) -> &'a Function {
        self.function.to_ref()
    }

    pub(super) fn ty_args(&self) -> &[Type] {
        &self.ty_args
    }

    pub(super) fn location(&self) -> Location {
        Location::Module(self.function().module_id().clone())
    }
}

impl ModuleDefinitionResolver {
    //
    // Creation: From a set of Runtime VTables and a ModuleId.
    //

    pub fn new(vtables: &VMDispatchTables, module_id: &ModuleId) -> PartialVMResult<Self> {
        let module = vtables.resolve_loaded_module(module_id)?;
        Ok(Self { module })
    }

    //
    // Function resolution
    //

    pub(crate) fn function_from_instantiation(&self, idx: FunctionInstantiationIndex) -> &CallType {
        &self.module.function_instantiation_at(idx.0).handle
    }

    //
    // Type lookup and instantiation
    //

    pub(crate) fn instantiate_generic_function(
        &self,
        idx: FunctionInstantiationIndex,
        type_params: &[Type],
    ) -> PartialVMResult<Vec<Type>> {
        let loaded_module = &*self.module;
        let func_inst = loaded_module.function_instantiation_at(idx.0);
        let instantiation: Vec<_> = func_inst
            .instantiation_signature
            .to_ref()
            .iter()
            .map(|ty| subst(ty, type_params))
            .collect::<PartialVMResult<_>>()?;

        // Check if the function instantiation over all generics is larger
        // than MAX_TYPE_INSTANTIATION_NODES.
        let mut sum_nodes = 1u64;
        for ty in type_params.iter().chain(instantiation.iter()) {
            sum_nodes = sum_nodes.saturating_add(count_type_nodes(ty));
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
        let struct_def = self.module.struct_at(idx);
        Type::Datatype(struct_def)
    }

    pub(crate) fn get_enum_type(&self, vidx: VariantHandleIndex) -> Type {
        let variant_handle = self.module.variant_handle_at(vidx);
        let enum_def = self.module.enum_at(variant_handle.enum_def);
        Type::Datatype(enum_def)
    }

    pub(crate) fn instantiate_struct_type(
        &self,
        idx: StructDefInstantiationIndex,
        ty_args: &[Type],
    ) -> PartialVMResult<Type> {
        let loaded_module = &*self.module;
        let struct_inst = loaded_module.struct_instantiation_at(idx.0);
        let instantiation = &struct_inst.instantiation_signature.to_ref();
        self.instantiate_type_common(&struct_inst.def, instantiation, ty_args)
    }

    pub(crate) fn instantiate_enum_type(
        &self,
        vidx: VariantInstantiationHandleIndex,
        ty_args: &[Type],
    ) -> PartialVMResult<Type> {
        let loaded_module = &*self.module;
        let handle = loaded_module.variant_instantiation_handle_at(vidx);
        let enum_inst = loaded_module.enum_instantiation_at(handle.enum_def);
        let instantiation = &enum_inst.instantiation_signature.to_ref();
        self.instantiate_type_common(&enum_inst.def, instantiation, ty_args)
    }

    fn instantiate_type_common(
        &self,
        gt_idx: &VirtualTableKey,
        type_params: &[Type],
        ty_args: &[Type],
    ) -> PartialVMResult<Type> {
        // Before instantiating the type, count the # of nodes of all type arguments plus
        // existing type instantiation.
        // If that number is larger than MAX_TYPE_INSTANTIATION_NODES, refuse to construct this type.
        // This prevents constructing larger and larger types via datatype instantiation.
        let mut sum_nodes = 1u64;
        for ty in ty_args.iter().chain(type_params.iter()) {
            sum_nodes = sum_nodes.saturating_add(count_type_nodes(ty));
            if sum_nodes > MAX_TYPE_INSTANTIATION_NODES {
                return Err(PartialVMError::new(StatusCode::TOO_MANY_TYPE_NODES));
            }
        }

        Ok(Type::DatatypeInstantiation(Box::new((
            gt_idx.clone(),
            type_params
                .iter()
                .map(|ty| subst(ty, ty_args))
                .collect::<PartialVMResult<_>>()?,
        ))))
    }

    #[allow(dead_code)]
    fn single_type_at(&self, idx: SignatureIndex) -> &Type {
        self.module.single_type_at(idx)
    }

    pub(crate) fn instantiate_single_type(
        &self,
        ty_ptr: &VMPointer<Type>,
        ty_args: &[Type],
    ) -> PartialVMResult<Type> {
        let ty = ty_ptr.to_ref();
        if !ty_args.is_empty() {
            subst(ty, ty_args)
        } else {
            Ok(ty.clone())
        }
    }

    //
    // Fields resolution
    //

    pub(crate) fn field_offset(&self, idx: FieldHandleIndex) -> usize {
        self.module.field_offset(idx)
    }

    pub(crate) fn field_instantiation_offset(&self, idx: FieldInstantiationIndex) -> usize {
        self.module.field_instantiation_offset(idx)
    }

    pub(crate) fn field_count(&self, idx: StructDefinitionIndex) -> u16 {
        self.module.field_count(idx.0)
    }

    pub(crate) fn variant_field_count_and_tag(
        &self,
        vidx: VariantHandleIndex,
    ) -> (u16, VariantTag) {
        self.module.variant_field_count(vidx)
    }

    pub(crate) fn field_instantiation_count(&self, idx: StructDefInstantiationIndex) -> u16 {
        self.module.field_instantiation_count(idx.0)
    }

    pub(crate) fn variant_instantiantiation_field_count_and_tag(
        &self,
        vidx: VariantInstantiationHandleIndex,
    ) -> (u16, VariantTag) {
        self.module
            .variant_instantiantiation_field_count_and_tag(vidx)
    }
}

// -------------------------------------------------------------------------------------------------
// Other impls
// -------------------------------------------------------------------------------------------------

impl<'a, 'b> TypeView for ResolvableType<'a, 'b> {
    fn to_type_tag(&self) -> TypeTag {
        self.vtables.type_to_type_tag(self.ty).unwrap()
    }
}
