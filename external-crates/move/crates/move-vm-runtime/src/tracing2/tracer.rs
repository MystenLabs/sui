// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    interpreter::{Frame, Interpreter},
    loader::{Function, Loader},
};
use move_binary_format::{
    errors::{PartialVMError, VMError, VMResult},
    file_format::{ConstantPoolIndex, SignatureIndex},
};
use move_core_types::{
    account_address::AccountAddress,
    annotated_value::{MoveTypeLayout, MoveValue},
    language_storage::TypeTag,
};
use move_trace_format::format::{
    DataLoad, Effect as EF, Location, MoveTraceBuilder, Read, RefType, TraceIndex, TraceValue,
    TypeTagWithRefs, Write,
};
use move_vm_types::{loaded_data::runtime_types::Type, values::Value};
use smallvec::SmallVec;
use std::collections::BTreeMap;

/// Internal state for the tracer. This is where the actual tracing logic is implemented.
pub(crate) struct VMTracer<'a> {
    trace: &'a mut MoveTraceBuilder,
    link_context: Option<AccountAddress>,
    pc: Option<u16>,
    active_frames: BTreeMap<TraceIndex, FrameInfo>,
    type_stack: Vec<RootedType>,
    loaded_data: BTreeMap<TraceIndex, TraceValue>,
    effects: Vec<EF>,
}

/// Information about a frame that we keep during trace building
#[derive(Debug, Clone)]
struct FrameInfo {
    frame_identifier: TraceIndex,
    is_native: bool,
    locals_types: Vec<LocalType>,
    return_types: Vec<TagWithLayoutInfoOpt>,
}

/// A type tag, and the move type layout and reference information for that type if it is
/// computable without error. Due to runtime value depth restrictions you can have a valid type
/// whose type layout is not computable at runtime without error.
#[derive(Debug, Clone)]
struct TagWithLayoutInfoOpt {
    tag: TypeTag,
    layout: (Option<MoveTypeLayout>, Option<RefType>),
}

// Information about a function that we use for trace building
// All types are fully substituted
#[derive(Debug, Clone)]
struct FunctionTypeInfo {
    ty_args: Vec<TypeTag>,
    local_types: Vec<TagWithLayoutInfoOpt>,
    return_types: Vec<TagWithLayoutInfoOpt>,
}

/// A runtime location can refer to the stack to make it easier to refer to values on the stack and
/// resolving them. However, the stack is not a valid location for a reference and all references
/// are rooted in a local or global so the Trace `Location` does not include the stack, and
/// only `Local`, `Global`, and `Indexed` locations.
#[derive(Debug, Clone)]
enum RuntimeLocation {
    Stack(usize),
    Local(TraceIndex, usize),
    Indexed(Box<RuntimeLocation>, usize),
    Global(TraceIndex),
}

/// The reference information for a local. This is used to track the state of a local in a frame.
/// * It can be a value, in which case the reference type is `Value`.
/// * It can be a local that does not currently hold a value (is "empty"), in which case
///   we track the reference type and the type of the local, but we don't have a `RuntimeLocation`
///   for the reference. This is e.g., the case when we open a frame and the local is not
///   initialized yet.
/// * It can be a local that holds a value (is "filled"), in which case we track the reference type and the
///   location the reference resolves to.
#[derive(Debug, Clone)]
enum ReferenceType {
    Value,
    Empty {
        ref_type: RefType,
    },
    Filled {
        ref_type: RefType,
        location: RuntimeLocation,
    },
}

/// A `RootedType` is a a type layout with reference information, where any reference type is
/// fully rooted back to a specific location.
#[derive(Debug, Clone)]
struct RootedType {
    layout: MoveTypeLayout,
    ref_type: Option<(RefType, RuntimeLocation)>,
}

/// A `LocalType` layout where a reference type may not be rooted to a
/// specific location (or it may be rooted to a specific location if the location is filled with a
/// value at the time). Note the type layout may be `None` in the case where the type is not
/// calculable at runtime without error.
#[derive(Debug, Clone)]
struct LocalType {
    layout: Option<MoveTypeLayout>,
    ref_type: ReferenceType,
}

impl TagWithLayoutInfoOpt {
    pub fn as_tag_with_refs(&self) -> TypeTagWithRefs {
        TypeTagWithRefs {
            type_: self.tag.clone(),
            ref_type: self.layout.1.clone(),
        }
    }
}

impl RuntimeLocation {
    fn as_trace_location(&self) -> Location {
        match self {
            RuntimeLocation::Stack(_) => {
                panic!("Cannot convert stack location to trace location")
            }
            RuntimeLocation::Local(fidx, lidx) => Location::Local(*fidx, *lidx),
            RuntimeLocation::Indexed(loc, idx) => {
                Location::Indexed(Box::new(loc.as_trace_location()), *idx)
            }
            RuntimeLocation::Global(id) => Location::Global(*id),
        }
    }

    fn as_runtime_location(loc: Location) -> Self {
        match loc {
            Location::Local(fidx, lidx) => RuntimeLocation::Local(fidx, lidx),
            Location::Indexed(loc, idx) => {
                RuntimeLocation::Indexed(Box::new(RuntimeLocation::as_runtime_location(*loc)), idx)
            }
            Location::Global(id) => RuntimeLocation::Global(id),
        }
    }
}

impl LocalType {
    fn into_rooted_type(self) -> Option<RootedType> {
        let ref_type = match self.ref_type {
            ReferenceType::Value => None,
            ReferenceType::Empty { .. } => panic!("Empty reference type"),
            ReferenceType::Filled { ref_type, location } => Some((ref_type, location)),
        };
        Some(RootedType {
            layout: self.layout?,
            ref_type,
        })
    }
}

impl RootedType {
    fn into_local_type(self) -> LocalType {
        let ref_type = match self.ref_type {
            None => ReferenceType::Value,
            Some((ref_type, location)) => ReferenceType::Filled { ref_type, location },
        };
        LocalType {
            layout: Some(self.layout),
            ref_type,
        }
    }
}

impl<'a> VMTracer<'a> {
    /// Emit an error event to the trace if `true`
    fn emit_trace_error_if_err(&mut self, is_err: bool) {
        if is_err {
            self.trace.effect(EF::ExecutionError(
                "!! TRACING ERROR !! Events below this may be incorrect.".to_string(),
            ));
        }
    }

    fn current_frame(&self) -> Option<&FrameInfo> {
        self.active_frames.last_key_value().map(|(_, v)| v)
    }

    fn current_frame_mut(&mut self) -> Option<&mut FrameInfo> {
        self.active_frames.last_entry().map(|e| e.into_mut())
    }

    /// Get the current locals type and reference state(s)
    fn current_frame_locals(&self) -> Option<&[LocalType]> {
        Some(self.current_frame()?.locals_types.as_slice())
    }

    /// Return the current frame identifier. This is trace index of the frame and is used to
    /// identify reference locations rooted higher up the call stack.
    fn current_frame_identifier(&self) -> Option<TraceIndex> {
        Some(self.current_frame()?.frame_identifier)
    }

    /// Given the trace index for a frame, return the index of the frame in the call stack.
    fn trace_index_to_frame_index(&self, idx: TraceIndex) -> Option<usize> {
        self.active_frames
            .range(..=idx)
            .enumerate()
            .last()
            .map(|(i, _)| i)
    }

    /// Register the pre-effects for the instruction (i.e., reads, pops.)
    fn register_pre_effects(&mut self, effects: Vec<EF>) {
        assert!(self.effects.is_empty());
        self.effects = effects;
    }

    /// Register the post-effects for the instruction (i.e., pushes, writes) and return the total
    /// effects for the instruction.
    fn register_post_effects(&mut self, effects: Vec<EF>) -> Vec<EF> {
        self.effects.extend(effects);
        std::mem::take(&mut self.effects)
    }

    /// Insert a local with a specifice runtime location into the current frame.
    fn insert_local(&mut self, local_index: usize, local: RootedType) -> Option<()> {
        *self
            .current_frame_mut()?
            .locals_types
            .get_mut(local_index)? = local.into_local_type();
        Some(())
    }

    /// Invalidate a local in the current frame. This is used to mark a local as uninitialized and
    /// remove its reference information.
    fn invalidate_local(&mut self, local_index: usize) -> Option<()> {
        let local = self
            .current_frame_mut()?
            .locals_types
            .get_mut(local_index)?;
        match &local.ref_type {
            ReferenceType::Filled { ref_type, .. } => {
                local.ref_type = ReferenceType::Empty {
                    ref_type: ref_type.clone(),
                }
            }
            ReferenceType::Empty { .. } => (),
            ReferenceType::Value => (),
        };
        Some(())
    }

    /// Resolve a value on the stack to a TraceValue. References are fully rooted all the way back
    /// to their location in a local.
    fn resolve_stack_value(
        &self,
        frame: Option<&Frame>,
        interpreter: &Interpreter,
        stack_idx: usize,
    ) -> Option<TraceValue> {
        if stack_idx >= interpreter.operand_stack.value.len() {
            return None;
        }
        let offset = self.type_stack.len() - 1;
        self.resolve_location(
            &RuntimeLocation::Stack(offset - stack_idx),
            frame,
            interpreter,
        )
    }

    /// Resolve a value in a local to a TraceValue. References are fully rooted all the way back to
    /// their root location in a local.
    fn resolve_local(
        &self,
        frame: &Frame,
        interpreter: &Interpreter,
        local_index: usize,
    ) -> Option<TraceValue> {
        self.resolve_location(
            &RuntimeLocation::Local(self.current_frame_identifier()?, local_index),
            Some(frame),
            interpreter,
        )
    }

    /// Shared utility function that creates a TraceValue from a runtime location along with
    /// grabbing the snapshot of the value.
    fn make_trace_value(
        &self,
        location: RuntimeLocation,
        ref_info: Option<RefType>,
        frame: Option<&Frame>,
        interpreter: &Interpreter,
    ) -> Option<TraceValue> {
        let value = self.root_location_snapshot(&location, frame, interpreter)?;
        Some(match ref_info {
            Some(RefType::Imm) => TraceValue::ImmRef {
                location: location.as_trace_location(),
                snapshot: Box::new(value),
            },
            Some(RefType::Mut) => TraceValue::MutRef {
                location: location.as_trace_location(),
                snapshot: Box::new(value),
            },
            None => TraceValue::RuntimeValue { value },
        })
    }

    /// Given a location, resolve it to the value it points to or the value itself in the case
    /// where it's not a reference.
    fn resolve_location(
        &self,
        loc: &RuntimeLocation,
        frame: Option<&Frame>,
        interpreter: &Interpreter,
    ) -> Option<TraceValue> {
        Some(match loc {
            RuntimeLocation::Stack(sidx) => {
                let ty = self.type_stack.get(*sidx)?;
                let ref_ty = ty.ref_type.as_ref().map(|(r, _)| r.clone());
                let location = ty
                    .ref_type
                    .as_ref()
                    .map(|(_, l)| l.clone())
                    .unwrap_or_else(|| loc.clone());
                self.make_trace_value(location, ref_ty, frame, interpreter)?
            }
            RuntimeLocation::Local(fidx, lidx) => {
                let ty = &self.active_frames.get(fidx)?.locals_types.get(*lidx)?;
                let ref_ty = match &ty.ref_type {
                    ReferenceType::Value => None,
                    ReferenceType::Empty { ref_type } => Some(ref_type.clone()),
                    ReferenceType::Filled { ref_type, .. } => Some(ref_type.clone()),
                };
                let location = match &ty.ref_type {
                    ReferenceType::Filled { location, .. } => location.clone(),
                    ReferenceType::Value => loc.clone(),
                    _ => panic!(
                        "We tried to access a local that was not initialized at {:?}",
                        loc
                    ),
                };
                self.make_trace_value(location, ref_ty, frame, interpreter)?
            }
            RuntimeLocation::Indexed(location, _) => {
                self.resolve_location(location, frame, interpreter)?
            }
            RuntimeLocation::Global(id) => self.loaded_data.get(id)?.clone(),
        })
    }

    /// Snapshot the value at the root of a location. This is used to create the value snapshots
    /// for TraceValue references.
    fn root_location_snapshot(
        &self,
        loc: &RuntimeLocation,
        frame: Option<&Frame>,
        interpreter: &Interpreter,
    ) -> Option<MoveValue> {
        Some(match loc {
            RuntimeLocation::Local(fidx, loc_idx) => {
                let local_ty = self
                    .active_frames
                    .get(fidx)?
                    .locals_types
                    .get(*loc_idx)?
                    .clone();
                let call_stack_index = self.trace_index_to_frame_index(*fidx)?;
                match local_ty.ref_type {
                    ReferenceType::Value => {
                        let frame = if call_stack_index >= interpreter.call_stack.0.len() {
                            frame?
                        } else {
                            interpreter.call_stack.0.get(call_stack_index)?
                        };
                        frame
                            .locals
                            .copy_loc(*loc_idx)
                            .ok()?
                            .as_annotated_move_value_for_tracing_only(&local_ty.layout?)?
                    }
                    ReferenceType::Empty { .. } => {
                        panic!("We tried to access a local that was not initialized")
                    }
                    ReferenceType::Filled { location, .. } => {
                        self.root_location_snapshot(&location, frame, interpreter)?
                    }
                }
            }
            RuntimeLocation::Stack(stack_idx) => {
                let ty = self.type_stack.get(*stack_idx)?;
                match &ty.ref_type {
                    Some((_, location)) => {
                        self.root_location_snapshot(location, frame, interpreter)?
                    }
                    None => {
                        let value = interpreter.operand_stack.value.get(*stack_idx)?;
                        value.as_annotated_move_value_for_tracing_only(&ty.layout)?
                    }
                }
            }
            RuntimeLocation::Indexed(loc, _) => {
                self.root_location_snapshot(loc, frame, interpreter)?
            }
            RuntimeLocation::Global(id) => self.loaded_data.get(id)?.snapshot().clone(),
        })
    }

    fn link_context(&self) -> AccountAddress {
        self.link_context
            .expect("Link context always set by this point")
    }

    /// Load data returned by a native function into the tracer state.
    /// We also emit a data load event for the data loaded from the native function.
    fn load_data(
        &mut self,
        layout: &MoveTypeLayout,
        reftype: &Option<RefType>,
        value: &Value,
    ) -> Option<(RefType, RuntimeLocation)> {
        let value = value.as_annotated_move_value_for_tracing_only(layout)?;

        let Some(ref_type) = reftype else {
            return None;
        };

        // We treat any references coming out of a native as global reference.
        // This generally works fine as long as you don't have a native function returning a
        // mutable reference within a mutable reference passed-in.
        let id = self.trace.current_trace_offset();

        let location = RuntimeLocation::Global(id);

        self.trace.effect(EF::DataLoad(DataLoad {
            ref_type: ref_type.clone(),
            location: location.as_trace_location(),
            snapshot: value.clone(),
        }));
        let trace_value = match &ref_type {
            RefType::Imm => TraceValue::ImmRef {
                location: location.as_trace_location(),
                snapshot: Box::new(value),
            },
            RefType::Mut => TraceValue::MutRef {
                location: location.as_trace_location(),
                snapshot: Box::new(value),
            },
        };
        self.loaded_data.insert(id, trace_value);
        Some((ref_type.clone(), location))
    }

    /// Handle (and load) any data returned by a native function.
    fn handle_native_return(
        &mut self,
        function: &Function,
        interpreter: &Interpreter,
    ) -> Option<()> {
        assert!(function.is_native());
        let trace_frame = self.current_frame()?.clone();
        assert!(trace_frame.is_native);
        let len = interpreter.operand_stack.value.len();
        for (i, r_ty) in trace_frame.return_types.iter().cloned().enumerate() {
            let r_ty = r_ty.layout;
            let ref_type = self.load_data(
                r_ty.0.as_ref()?,
                &r_ty.1,
                interpreter.operand_stack.value.get(len - i - 1)?,
            );
            self.type_stack.push(RootedType {
                layout: r_ty.0?,
                ref_type,
            });
        }
        Some(())
    }

    //---------------------------------------------------------------------------
    // Core entry points for the tracer
    //---------------------------------------------------------------------------

    fn open_initial_frame_(
        &mut self,
        args: &[Value],
        ty_args: &[Type],
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    ) -> Option<()> {
        self.link_context = Some(link_context);

        let function_type_info = FunctionTypeInfo::new(function, loader, ty_args, link_context)?;

        assert!(function_type_info.local_types.len() == function.local_count());

        let call_args: Vec<_> = args
            .iter()
            .zip(function_type_info.local_types.iter().cloned())
            .map(|(value, tag_with_layout_info_opt)| {
                let (layout, ref_type) = tag_with_layout_info_opt.layout;
                let move_value = value.as_annotated_move_value_for_tracing_only(&layout?)?;
                assert!(ref_type.is_none());
                Some(TraceValue::RuntimeValue { value: move_value })
            })
            .collect::<Option<_>>()?;

        let locals_types = function_type_info
            .local_types
            .iter()
            .cloned()
            .map(|tag_with_layout_info_opt| {
                let (layout, ref_type) = tag_with_layout_info_opt.layout;
                LocalType {
                    layout,
                    ref_type: ref_type
                        .map(|r_type| match r_type {
                            RefType::Imm => ReferenceType::Empty { ref_type: r_type },
                            RefType::Mut => ReferenceType::Empty { ref_type: r_type },
                        })
                        .unwrap_or(ReferenceType::Value),
                }
            })
            .collect();

        let current_trace_offset = self.trace.current_trace_offset();
        self.active_frames.insert(
            current_trace_offset,
            FrameInfo {
                frame_identifier: current_trace_offset,
                is_native: function.is_native(),
                locals_types,
                return_types: function_type_info.return_types.clone(),
            },
        );

        self.trace.open_frame(
            self.current_frame_identifier()?,
            function.index(),
            function.name().to_string(),
            function.module_id().clone(),
            call_args,
            function_type_info.ty_args,
            function_type_info
                .return_types
                .iter()
                .map(|tag_with_layout_info_opt| tag_with_layout_info_opt.as_tag_with_refs())
                .collect(),
            function_type_info
                .local_types
                .into_iter()
                .map(|tag_with_layout_info_opt| tag_with_layout_info_opt.as_tag_with_refs())
                .collect(),
            function.is_native(),
            remaining_gas,
        );
        Some(())
    }

    fn close_initial_frame_(&mut self, return_values: &[Value], remaining_gas: u64) -> Option<()> {
        let current_frame_return_tys = self.current_frame()?.return_types.clone();
        let return_values: Vec<_> = return_values
            .iter()
            .zip(current_frame_return_tys.into_iter())
            .map(|(value, tag_with_layout_info_opt)| {
                let (layout, ref_type) = tag_with_layout_info_opt.layout;
                let move_value = value.as_annotated_move_value_for_tracing_only(&layout?)?;
                assert!(ref_type.is_none());
                Some(TraceValue::RuntimeValue { value: move_value })
            })
            .collect::<Option<_>>()?;
        self.trace.close_frame(
            self.current_frame_identifier()?,
            return_values,
            remaining_gas,
        );
        self.active_frames
            .pop_last()
            .expect("Unbalanced frame close");
        Some(())
    }

    fn open_frame_(
        &mut self,
        ty_args: &[Type],
        function: &Function,
        calling_frame: &Frame,
        interpreter: &Interpreter,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    ) -> Option<()> {
        self.link_context = Some(link_context);

        let call_args = (0..function.arg_count())
            .rev()
            .map(|i| self.resolve_stack_value(Some(calling_frame), interpreter, i))
            .collect::<Option<Vec<_>>>()?;

        let call_args_types = self
            .type_stack
            .split_off(self.type_stack.len() - function.arg_count());
        let function_type_info = FunctionTypeInfo::new(function, loader, ty_args, link_context)?;

        let locals_types = function_type_info
            .local_types
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, tag_with_layout_info_opt)| {
                // For any arguments, start them out with the correct locations
                if let Some(a_layout) = call_args_types.get(i).cloned() {
                    let ref_type = match a_layout.ref_type {
                        Some((ref_type, location)) => ReferenceType::Filled { ref_type, location },
                        None => ReferenceType::Value,
                    };
                    LocalType {
                        layout: Some(a_layout.layout),
                        ref_type,
                    }
                } else {
                    let (layout, ref_type) = tag_with_layout_info_opt.layout;
                    let ref_type = ref_type
                        .map(|ref_type| ReferenceType::Empty { ref_type })
                        .unwrap_or(ReferenceType::Value);
                    LocalType { layout, ref_type }
                }
            })
            .collect();

        let current_trace_offset = self.trace.current_trace_offset();
        self.active_frames.insert(
            current_trace_offset,
            FrameInfo {
                frame_identifier: current_trace_offset,
                is_native: function.is_native(),
                locals_types,
                return_types: function_type_info.return_types.clone(),
            },
        );

        self.trace.open_frame(
            self.current_frame_identifier()?,
            function.index(),
            function.name().to_string(),
            function.module_id().clone(),
            call_args,
            function_type_info.ty_args,
            function_type_info
                .return_types
                .iter()
                .map(|tag_with_layout_info_opt| tag_with_layout_info_opt.as_tag_with_refs())
                .collect(),
            function_type_info
                .local_types
                .into_iter()
                .map(|tag_with_layout_info_opt| tag_with_layout_info_opt.as_tag_with_refs())
                .collect(),
            function.is_native(),
            remaining_gas,
        );
        Some(())
    }

    fn close_frame_(
        &mut self,
        frame: &Frame,
        function: &Function,
        interpreter: &Interpreter,
        _loader: &Loader,
        remaining_gas: u64,
        _link_context: AccountAddress,
    ) -> Option<()> {
        if function.is_native() {
            self.handle_native_return(function, interpreter)
                .expect("Native function return failed -- this should not happen.");
        }

        let return_values = (0..function.return_type_count())
            .rev()
            .map(|i| self.resolve_stack_value(Some(frame), interpreter, i))
            .collect::<Option<Vec<_>>>()?;

        // Note that when a native function frame closes the values returned by the native function
        // are all pushed on the operand stack.
        if function.is_native() {
            for val in &return_values {
                self.trace.effect(EF::Push(val.clone()));
            }
        }

        self.trace.close_frame(
            self.current_frame_identifier()?,
            return_values,
            remaining_gas,
        );
        self.active_frames
            .pop_last()
            .expect("Unbalanced frame close");
        Some(())
    }

    fn open_instruction_(
        &mut self,
        frame: &Frame,
        interpreter: &Interpreter,
        loader: &Loader,
        _remaining_gas: u64,
    ) -> Option<()> {
        use move_binary_format::file_format::Bytecode as B;

        let pc = frame.pc;
        self.pc = Some(pc);

        let popn = |n: usize| {
            let mut effects = vec![];
            for i in 0..n {
                let v = self.resolve_stack_value(Some(frame), interpreter, i)?;
                effects.push(EF::Pop(v));
            }
            Some(effects)
        };

        assert_eq!(
            self.type_stack.len(),
            interpreter.operand_stack.value.len(),
            "Type stack and operand stack must be the same length {} {}",
            frame.function.name(),
            pc,
        );

        match &frame.function.code()[pc as usize] {
            B::Nop
            | B::Branch(_)
            | B::Ret
            | B::LdU8(_)
            | B::LdU16(_)
            | B::LdU32(_)
            | B::LdU64(_)
            | B::LdU128(_)
            | B::LdU256(_)
            | B::LdFalse
            | B::LdTrue
            | B::LdConst(_) => {
                self.register_pre_effects(vec![]);
            }
            B::MutBorrowField(_)
            | B::ImmBorrowField(_)
            | B::MutBorrowFieldGeneric(_)
            | B::ImmBorrowFieldGeneric(_)
            | B::FreezeRef
            | B::Not
            | B::Abort
            | B::Unpack(_)
            | B::UnpackGeneric(_)
            | B::CastU8
            | B::CastU16
            | B::CastU32
            | B::CastU64
            | B::CastU128
            | B::CastU256
            | B::Pop
            | B::BrTrue(_)
            | B::BrFalse(_)
            | B::VecUnpack(_, _)
            | B::VecLen(_)
            | B::VecPopBack(_)
            | B::VariantSwitch(_)
            | B::UnpackVariantImmRef(_)
            | B::UnpackVariantMutRef(_)
            | B::UnpackVariantGenericImmRef(_)
            | B::UnpackVariantGenericMutRef(_)
            | B::UnpackVariant(_)
            | B::UnpackVariantGeneric(_) => {
                self.register_pre_effects(popn(1)?);
            }
            B::Add
            | B::Sub
            | B::Mul
            | B::Mod
            | B::Div
            | B::BitOr
            | B::BitAnd
            | B::Xor
            | B::Shl
            | B::Shr
            | B::Lt
            | B::Gt
            | B::Le
            | B::Ge
            | B::Eq
            | B::Neq
            | B::Or
            | B::And
            | B::WriteRef
            | B::VecImmBorrow(_)
            | B::VecMutBorrow(_)
            | B::VecPushBack(_) => self.register_pre_effects(popn(2)?),
            B::VecSwap(_) => self.register_pre_effects(popn(3)?),
            B::VecPack(_, n) => self.register_pre_effects(popn(*n as usize)?),
            i @ (B::MoveLoc(l) | B::CopyLoc(l)) => {
                let v = self.resolve_local(frame, interpreter, *l as usize)?;
                let effects = vec![EF::Read(Read {
                    location: Location::Local(self.current_frame_identifier()?, *l as usize),
                    root_value_read: v.clone(),
                    moved: matches!(i, B::MoveLoc(_)),
                })];
                self.register_pre_effects(effects);
            }
            B::StLoc(lidx) => {
                let ty = self.type_stack.last()?;
                let v = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                self.insert_local(*lidx as usize, ty.clone())?;
                let effects = vec![EF::Pop(v.clone())];
                self.register_pre_effects(effects);
            }
            B::ImmBorrowLoc(l_idx) | B::MutBorrowLoc(l_idx) => {
                let val = self.resolve_local(frame, interpreter, *l_idx as usize)?;
                let location = Location::Local(self.current_frame_identifier()?, *l_idx as usize);
                self.register_pre_effects(vec![EF::Read(Read {
                    location,
                    root_value_read: val,
                    moved: false,
                })]);
            }
            // Handled by open frame
            B::Call(_) | B::CallGeneric(_) => {}
            B::Pack(sidx) => {
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let field_count = resolver.field_count(*sidx) as usize;
                self.register_pre_effects(popn(field_count)?);
            }
            B::PackGeneric(sidx) => {
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let field_count = resolver.field_instantiation_count(*sidx) as usize;
                self.register_pre_effects(popn(field_count)?);
            }
            B::PackVariant(vidx) => {
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let (field_count, _variant_tag) = resolver.variant_field_count_and_tag(*vidx);
                self.register_pre_effects(popn(field_count as usize)?);
            }
            B::PackVariantGeneric(vidx) => {
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let (field_count, _variant_tag) =
                    resolver.variant_instantiantiation_field_count_and_tag(*vidx);
                self.register_pre_effects(popn(field_count as usize)?);
            }
            B::ReadRef => {
                let ref_value = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let location = ref_value.location()?.clone();
                let runtime_location = RuntimeLocation::as_runtime_location(location.clone());
                let value = self.resolve_location(&runtime_location, Some(frame), interpreter)?;
                self.register_pre_effects(vec![
                    EF::Pop(ref_value),
                    EF::Read(Read {
                        location,
                        root_value_read: value.clone(),
                        moved: false,
                    }),
                ]);
            }

            B::ExistsDeprecated(_)
            | B::ExistsGenericDeprecated(_)
            | B::MoveFromDeprecated(_)
            | B::MoveFromGenericDeprecated(_)
            | B::MoveToDeprecated(_)
            | B::MoveToGenericDeprecated(_)
            | B::MutBorrowGlobalDeprecated(_)
            | B::MutBorrowGlobalGenericDeprecated(_)
            | B::ImmBorrowGlobalDeprecated(_)
            | B::ImmBorrowGlobalGenericDeprecated(_) => unreachable!(),
        }
        Some(())
    }

    fn close_instruction_(
        &mut self,
        frame: &Frame,
        interpreter: &Interpreter,
        loader: &Loader,
        remaining_gas: u64,
    ) -> Option<()> {
        use move_binary_format::file_format::Bytecode as B;

        // NB: Do _not_ use the frames pc here, as it will be incremented by the interpreter to the
        // next instruction already.
        let pc = self
            .pc
            .expect("PC always set by this point by `open_instruction`");

        // NB: At the start of this function (i.e., at this point) the operand stack in the VM, and
        // the type stack in the tracer are _out of sync_. This is because the VM has already
        // executed the instruction and we now need to manage the type transition of the
        // instruction along with snapshoting the effects of the instruction's execution.
        let instruction = &frame.function.code()[pc as usize];
        match instruction {
            B::Pop | B::BrTrue(_) | B::BrFalse(_) => {
                self.type_stack.pop()?;
                let effects = self.register_post_effects(vec![]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::Branch(_) | B::Ret => {
                let effects = self.register_post_effects(vec![]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            i @ (B::LdU8(_)
            | B::LdU16(_)
            | B::LdU32(_)
            | B::LdU64(_)
            | B::LdU128(_)
            | B::LdU256(_)
            | B::LdFalse
            | B::LdTrue
            | B::LdConst(_)) => {
                let layout = match i {
                    B::LdU8(_) => MoveTypeLayout::U8,
                    B::LdU16(_) => MoveTypeLayout::U16,
                    B::LdU32(_) => MoveTypeLayout::U32,
                    B::LdU64(_) => MoveTypeLayout::U64,
                    B::LdU128(_) => MoveTypeLayout::U128,
                    B::LdU256(_) => MoveTypeLayout::U256,
                    B::LdTrue => MoveTypeLayout::Bool,
                    B::LdFalse => MoveTypeLayout::Bool,
                    B::LdConst(const_idx) => get_constant_type_layout(
                        &frame.function,
                        loader,
                        self.link_context(),
                        *const_idx,
                    )?,
                    _ => unreachable!(),
                };
                let a_layout = RootedType {
                    layout,
                    ref_type: None,
                };
                self.type_stack.push(a_layout);

                let value = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = vec![EF::Push(value)];
                let effects = self.register_post_effects(effects);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            i @ (B::MoveLoc(l) | B::CopyLoc(l)) => {
                let local_annot_type = self
                    .current_frame_locals()?
                    .get(*l as usize)?
                    .clone()
                    .into_rooted_type()?;
                self.type_stack.push(local_annot_type);
                if matches!(i, B::MoveLoc(_)) {
                    self.invalidate_local(*l as usize)?;
                }
                // This was pushed on the stack during execution so read it off from there.
                let v = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(v.clone())]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            i @ (B::CastU8 | B::CastU16 | B::CastU32 | B::CastU64 | B::CastU128 | B::CastU256) => {
                let layout = match i {
                    B::CastU8 => MoveTypeLayout::U8,
                    B::CastU16 => MoveTypeLayout::U16,
                    B::CastU32 => MoveTypeLayout::U32,
                    B::CastU64 => MoveTypeLayout::U64,
                    B::CastU128 => MoveTypeLayout::U128,
                    B::CastU256 => MoveTypeLayout::U256,
                    _ => unreachable!(),
                };
                let annot_layout = RootedType {
                    layout,
                    ref_type: None,
                };
                self.type_stack.pop()?;
                self.type_stack.push(annot_layout);

                let value = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = vec![EF::Push(value.clone())];
                let effects = self.register_post_effects(effects);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::StLoc(lidx) => {
                let ty = self.type_stack.pop()?;
                self.insert_local(*lidx as usize, ty.clone())?;
                let v = self.resolve_local(frame, interpreter, *lidx as usize)?;
                let effects = self.register_post_effects(vec![EF::Write(Write {
                    location: Location::Local(self.current_frame_identifier()?, *lidx as usize),
                    root_value_after_write: v.clone(),
                })]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::Add
            | B::Sub
            | B::Mul
            | B::Mod
            | B::Div
            | B::BitOr
            | B::BitAnd
            | B::Xor
            | B::Shl
            | B::Shr => {
                self.type_stack.pop()?;
                // NB in the case of shift left and shift right the second operand is the resultant
                // value type.
                let a_ty = self.type_stack.pop()?;
                self.type_stack.push(a_ty);

                let result = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(result)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::Lt | B::Gt | B::Le | B::Ge => {
                self.type_stack.pop()?;
                self.type_stack.pop()?;
                let a_layout = RootedType {
                    layout: MoveTypeLayout::Bool,
                    ref_type: None,
                };
                self.type_stack.push(a_layout);

                let value = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(value)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::Call(_) | B::CallGeneric(_) => {
                // NB: We don't register effects for calls as they will be handled by
                // open_frame.
                self.trace
                    .instruction(instruction, vec![], vec![], remaining_gas, pc);
            }
            B::Pack(sidx) => {
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let field_count = resolver.field_count(*sidx) as usize;
                let struct_type = resolver.get_struct_type(*sidx);
                let stack_len = self.type_stack.len();
                let _ = self.type_stack.split_off(stack_len - field_count);
                let ty = loader.type_to_fully_annotated_layout(&struct_type).ok()?;
                let a_layout = RootedType {
                    layout: ty,
                    ref_type: None,
                };
                self.type_stack.push(a_layout);

                let value = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(value)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::PackGeneric(sidx) => {
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let field_count = resolver.field_instantiation_count(*sidx) as usize;
                let struct_type = resolver
                    .instantiate_struct_type(*sidx, &frame.ty_args)
                    .ok()?;
                let stack_len = self.type_stack.len();
                let _ = self.type_stack.split_off(stack_len - field_count);
                let ty = loader.type_to_fully_annotated_layout(&struct_type).ok()?;
                let a_layout = RootedType {
                    layout: ty,
                    ref_type: None,
                };
                self.type_stack.push(a_layout);

                let value = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(value)]);
                let TypeTag::Struct(s_type) = loader.type_to_type_tag(&struct_type).ok()? else {
                    panic!("Expected struct, got {:#?}", struct_type);
                };
                self.trace
                    .instruction(instruction, s_type.type_params, effects, remaining_gas, pc);
            }
            B::Unpack(_) | B::UnpackGeneric(_) => {
                let ty = self.type_stack.pop()?;
                let MoveTypeLayout::Struct(s) = ty.layout else {
                    panic!("Expected struct, got {:#?}", ty.layout);
                };
                let field_tys = s.fields.iter().map(|t| t.layout.clone());
                for field_ty in field_tys {
                    self.type_stack.push(RootedType {
                        layout: field_ty.clone(),
                        ref_type: None,
                    });
                }

                let mut effects = vec![];
                for i in (0..s.fields.len()).rev() {
                    let value = self.resolve_stack_value(Some(frame), interpreter, i)?;
                    effects.push(EF::Push(value));
                }

                let effects = self.register_post_effects(effects);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::Eq | B::Neq => {
                self.type_stack.pop()?;
                self.type_stack.pop()?;
                let a_layout = RootedType {
                    layout: MoveTypeLayout::Bool,
                    ref_type: None,
                };
                self.type_stack.push(a_layout);
                let value = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(value)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::Or | B::And => {
                self.type_stack.pop()?;
                self.type_stack.pop()?;
                let a_layout = RootedType {
                    layout: MoveTypeLayout::Bool,
                    ref_type: None,
                };
                self.type_stack.push(a_layout);
                let value = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(value)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::Not => {
                let a_ty = self.type_stack.pop()?;
                self.type_stack.push(a_ty);
                let value = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(value)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::Nop => {
                self.trace
                    .instruction(instruction, vec![], vec![], remaining_gas, pc);
            }
            B::Abort => {
                self.type_stack.pop()?;
                let effects = self.register_post_effects(vec![]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::ReadRef => {
                let ref_ty = self.type_stack.pop()?;
                let a_layout = RootedType {
                    layout: ref_ty.layout.clone(),
                    ref_type: None,
                };
                self.type_stack.push(a_layout);

                let value = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(value)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            i @ (B::ImmBorrowLoc(l_idx) | B::MutBorrowLoc(l_idx)) => {
                let non_imm_ty = self.current_frame_locals()?.get(*l_idx as usize)?.clone();
                let ref_type = match i {
                    B::ImmBorrowLoc(_) => RefType::Imm,
                    B::MutBorrowLoc(_) => RefType::Mut,
                    _ => unreachable!(),
                };
                let a_layout = RootedType {
                    layout: non_imm_ty.layout?.clone(),
                    ref_type: Some((
                        ref_type,
                        RuntimeLocation::Local(self.current_frame_identifier()?, *l_idx as usize),
                    )),
                };
                self.type_stack.push(a_layout);

                let val = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(val)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::WriteRef => {
                let reference_ty = self.type_stack.pop()?;
                let _value_ty = self.type_stack.pop()?;
                let location = reference_ty.ref_type.as_ref()?.1.clone();
                let root_value_after_write = self
                    .resolve_location(&location, Some(frame), interpreter)?
                    .clone();
                let effects = self.register_post_effects(vec![EF::Write(Write {
                    location: location.as_trace_location(),
                    root_value_after_write,
                })]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::FreezeRef => {
                let mut reference_ty = self.type_stack.pop()?;
                reference_ty.ref_type.as_mut()?.0 = RefType::Imm;
                self.type_stack.push(reference_ty);
                let reference_val = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(reference_val)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            i @ (B::MutBorrowField(fhidx) | B::ImmBorrowField(fhidx)) => {
                let value_ty = self.type_stack.pop()?;

                let MoveTypeLayout::Struct(slayout) = &value_ty.layout else {
                    panic!("Expected struct, got {:?}", value_ty.layout)
                };
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let field_offset = resolver.field_offset(*fhidx);
                let field_layout = slayout.fields.get(field_offset)?.layout.clone();

                let location = value_ty.ref_type.as_ref()?.1.clone();
                let field_location =
                    RuntimeLocation::Indexed(Box::new(location.clone()), field_offset);

                let ref_type = match i {
                    B::MutBorrowField(_) => RefType::Mut,
                    B::ImmBorrowField(_) => RefType::Imm,
                    _ => unreachable!(),
                };
                let a_layout = RootedType {
                    layout: field_layout,
                    ref_type: Some((ref_type, field_location)),
                };
                self.type_stack.push(a_layout);
                let value = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(value)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            i @ (B::MutBorrowFieldGeneric(fhidx) | B::ImmBorrowFieldGeneric(fhidx)) => {
                let value_ty = self.type_stack.pop()?;

                let MoveTypeLayout::Struct(slayout) = &value_ty.layout else {
                    panic!("Expected struct, got {:?}", value_ty.layout)
                };
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let field_offset = resolver.field_instantiation_offset(*fhidx);
                let field_layout = slayout.fields.get(field_offset)?.layout.clone();
                let location = value_ty.ref_type.as_ref()?.1.clone();
                let field_location =
                    RuntimeLocation::Indexed(Box::new(location.clone()), field_offset);

                let ref_type = match i {
                    B::MutBorrowFieldGeneric(_) => RefType::Mut,
                    B::ImmBorrowFieldGeneric(_) => RefType::Imm,
                    _ => unreachable!(),
                };
                let a_layout = RootedType {
                    layout: field_layout,
                    ref_type: Some((ref_type, field_location)),
                };
                self.type_stack.push(a_layout);
                let value = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(value)]);
                let ty_args = slayout.type_.type_params.clone();
                self.trace
                    .instruction(instruction, ty_args, effects, remaining_gas, pc);
            }

            B::VecPack(tok, n) => {
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let ty = resolver
                    .instantiate_single_type(*tok, &frame.ty_args)
                    .ok()?;
                let ty = loader.type_to_fully_annotated_layout(&ty).ok()?;
                let ty = MoveTypeLayout::Vector(Box::new(ty));
                let stack_len = self.type_stack.len();
                let _ = self.type_stack.split_off(stack_len - *n as usize);
                let a_layout = RootedType {
                    layout: ty,
                    ref_type: None,
                };
                self.type_stack.push(a_layout);
                let val = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(val)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            i @ (B::VecImmBorrow(_) | B::VecMutBorrow(_)) => {
                let ref_type = match i {
                    B::VecImmBorrow(_) => RefType::Imm,
                    B::VecMutBorrow(_) => RefType::Mut,
                    _ => unreachable!(),
                };
                self.type_stack.pop()?;
                let ref_ty = self.type_stack.pop()?;
                let MoveTypeLayout::Vector(ty) = ref_ty.layout else {
                    panic!("Expected vector, got {:?}", ref_ty.layout,);
                };
                let EF::Pop(TraceValue::RuntimeValue {
                    value: MoveValue::U64(i),
                }) = &self.effects[0]
                else {
                    unreachable!();
                };
                let location =
                    RuntimeLocation::Indexed(Box::new(ref_ty.ref_type?.1.clone()), *i as usize);
                let a_layout = RootedType {
                    layout: (*ty).clone(),
                    ref_type: Some((ref_type, location)),
                };
                self.type_stack.push(a_layout);
                let val = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(val)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::VecLen(_) => {
                self.type_stack.pop()?;
                let a_layout = RootedType {
                    layout: MoveTypeLayout::U64,
                    ref_type: None,
                };
                self.type_stack.push(a_layout);
                let len = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(len)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::VecPushBack(_) => {
                self.type_stack.pop()?;
                self.type_stack.pop()?;
                let EF::Pop(reference_val) = &self.effects[1] else {
                    unreachable!();
                };
                let location = reference_val.location()?.clone();
                let runtime_location = RuntimeLocation::as_runtime_location(location.clone());
                let snap = self.resolve_location(&runtime_location, Some(frame), interpreter)?;
                let effects = self.register_post_effects(vec![EF::Write(Write {
                    location,
                    root_value_after_write: snap,
                })]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::VecPopBack(_) => {
                let reference_ty = self.type_stack.pop()?;
                let MoveTypeLayout::Vector(ty) = reference_ty.layout else {
                    panic!("Expected vector, got {:?}", reference_ty.layout);
                };
                let a_layout = RootedType {
                    layout: (*ty).clone(),
                    ref_type: None,
                };
                self.type_stack.push(a_layout);
                let v = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(v)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::VecUnpack(_, n) => {
                let ty = self.type_stack.pop()?;
                let MoveTypeLayout::Vector(ty) = ty.layout else {
                    panic!("Expected vector, got {:?}", ty.layout);
                };
                for _ in 0..*n {
                    let a_layout = RootedType {
                        layout: (*ty).clone(),
                        ref_type: None,
                    };
                    self.type_stack.push(a_layout);
                }
                let mut effects = vec![];
                for i in (0..*n).rev() {
                    let value = self.resolve_stack_value(Some(frame), interpreter, i as usize)?;
                    effects.push(EF::Push(value));
                }
                let effects = self.register_post_effects(effects);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::VecSwap(_) => {
                self.type_stack.pop()?;
                self.type_stack.pop()?;
                let v_ref = self.type_stack.pop()?;
                let location = v_ref.ref_type.as_ref()?.1.clone();
                let snap = self.resolve_location(&location, Some(frame), interpreter)?;
                let effects = self.register_post_effects(vec![EF::Write(Write {
                    location: location.as_trace_location(),
                    root_value_after_write: snap,
                })]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::PackVariant(vidx) => {
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let (field_count, _variant_tag) = resolver.variant_field_count_and_tag(*vidx);
                let stack_len = self.type_stack.len();
                let _ = self.type_stack.split_off(stack_len - field_count as usize);
                let ty = loader
                    .type_to_fully_annotated_layout(&resolver.get_enum_type(*vidx))
                    .ok()?;
                let a_layout = RootedType {
                    layout: ty,
                    ref_type: None,
                };
                self.type_stack.push(a_layout);
                let val = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(val)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::PackVariantGeneric(vidx) => {
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let (field_count, _variant_tag) =
                    resolver.variant_instantiantiation_field_count_and_tag(*vidx);
                let stack_len = self.type_stack.len();
                let _ = self.type_stack.split_off(stack_len - field_count as usize);
                let ty = loader
                    .type_to_fully_annotated_layout(
                        &resolver.instantiate_enum_type(*vidx, &frame.ty_args).ok()?,
                    )
                    .ok()?;
                let a_layout = RootedType {
                    layout: ty,
                    ref_type: None,
                };
                self.type_stack.push(a_layout);
                let val = self.resolve_stack_value(Some(frame), interpreter, 0)?;
                let effects = self.register_post_effects(vec![EF::Push(val)]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            i @ (B::UnpackVariant(_) | B::UnpackVariantGeneric(_)) => {
                let ty = self.type_stack.pop()?;
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let (field_count, tag) = match i {
                    B::UnpackVariant(vidx) => resolver.variant_field_count_and_tag(*vidx),
                    B::UnpackVariantGeneric(vidx) => {
                        resolver.variant_instantiantiation_field_count_and_tag(*vidx)
                    }
                    _ => unreachable!(),
                };
                let MoveTypeLayout::Enum(e) = ty.layout else {
                    panic!("Expected enum, got {:#?}", ty.layout);
                };
                let variant_layout = e.variants.iter().find(|v| v.0 .1 == tag)?;
                let mut effects = vec![];
                for f_layout in variant_layout.1.iter() {
                    let a_layout = RootedType {
                        layout: f_layout.layout.clone(),
                        ref_type: None,
                    };
                    self.type_stack.push(a_layout);
                }
                for i in 0..field_count {
                    let value = self.resolve_stack_value(Some(frame), interpreter, i as usize)?;
                    effects.push(EF::Push(value));
                }
                let effects = self.register_post_effects(effects);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            i @ (B::UnpackVariantImmRef(_)
            | B::UnpackVariantMutRef(_)
            | B::UnpackVariantGenericImmRef(_)
            | B::UnpackVariantGenericMutRef(_)) => {
                let ty = self.type_stack.pop()?;
                let resolver = frame.function.get_resolver(self.link_context(), loader);
                let ((field_count, tag), ref_type) = match i {
                    B::UnpackVariantImmRef(vidx) => {
                        (resolver.variant_field_count_and_tag(*vidx), RefType::Imm)
                    }
                    B::UnpackVariantMutRef(vidx) => {
                        (resolver.variant_field_count_and_tag(*vidx), RefType::Mut)
                    }
                    B::UnpackVariantGenericImmRef(vidx) => (
                        resolver.variant_instantiantiation_field_count_and_tag(*vidx),
                        RefType::Imm,
                    ),
                    B::UnpackVariantGenericMutRef(vidx) => (
                        resolver.variant_instantiantiation_field_count_and_tag(*vidx),
                        RefType::Mut,
                    ),
                    _ => unreachable!(),
                };
                let MoveTypeLayout::Enum(e) = ty.layout else {
                    panic!("Expected enum, got {:#?}", ty.layout);
                };
                let variant_layout = e.variants.iter().find(|v| v.0 .1 == tag)?;
                let location = ty.ref_type.as_ref()?.1.clone();

                let mut effects = vec![];
                for (i, f_layout) in variant_layout.1.iter().enumerate() {
                    let location = RuntimeLocation::Indexed(Box::new(location.clone()), i);
                    let a_layout = RootedType {
                        layout: f_layout.layout.clone(),
                        ref_type: Some((ref_type.clone(), location)),
                    };
                    self.type_stack.push(a_layout);
                }
                for i in 0..field_count {
                    let value = self.resolve_stack_value(Some(frame), interpreter, i as usize)?;
                    effects.push(EF::Push(value));
                }
                let effects = self.register_post_effects(effects);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::VariantSwitch(_) => {
                self.type_stack.pop()?;
                let effects = self.register_post_effects(vec![]);
                self.trace
                    .instruction(instruction, vec![], effects, remaining_gas, pc);
            }
            B::ExistsDeprecated(_)
            | B::ExistsGenericDeprecated(_)
            | B::MoveFromDeprecated(_)
            | B::MoveFromGenericDeprecated(_)
            | B::MoveToDeprecated(_)
            | B::MoveToGenericDeprecated(_)
            | B::MutBorrowGlobalDeprecated(_)
            | B::MutBorrowGlobalGenericDeprecated(_)
            | B::ImmBorrowGlobalDeprecated(_)
            | B::ImmBorrowGlobalGenericDeprecated(_) => unreachable!(),
        }

        // At this point the type stack and the operand stack should be in sync.
        assert_eq!(self.type_stack.len(), interpreter.operand_stack.value.len());
        Some(())
    }
}

/// The (public crate) API for the VM tracer.
impl<'a> VMTracer<'a> {
    pub(crate) fn new(trace: &'a mut MoveTraceBuilder) -> Self {
        Self {
            trace,
            link_context: None,
            pc: None,
            active_frames: BTreeMap::new(),
            type_stack: vec![],
            loaded_data: BTreeMap::new(),
            effects: vec![],
        }
    }

    pub(crate) fn open_initial_frame(
        &mut self,
        args: &[Value],
        ty_args: &[Type],
        function: &Function,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    ) {
        let opt =
            self.open_initial_frame_(args, ty_args, function, loader, remaining_gas, link_context);
        self.emit_trace_error_if_err(opt.is_none());
    }

    pub(crate) fn close_initial_frame(
        &mut self,
        return_values: &VMResult<SmallVec<[Value; 1]>>,
        remaining_gas: u64,
    ) {
        let return_values = match return_values {
            Ok(values) => values,
            Err(err) => {
                self.trace
                    .effect(EF::ExecutionError(format!("{:?}", err.major_status())));
                return;
            }
        };
        let opt = self.close_initial_frame_(return_values, remaining_gas);
        self.emit_trace_error_if_err(opt.is_none());
    }

    pub(crate) fn open_frame(
        &mut self,
        ty_args: &[Type],
        function: &Function,
        calling_frame: &Frame,
        interpreter: &Interpreter,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
    ) {
        let opt = self.open_frame_(
            ty_args,
            function,
            calling_frame,
            interpreter,
            loader,
            remaining_gas,
            link_context,
        );
        self.emit_trace_error_if_err(opt.is_none())
    }

    pub(crate) fn close_frame(
        &mut self,
        frame: &Frame,
        function: &Function,
        interpreter: &Interpreter,
        loader: &Loader,
        remaining_gas: u64,
        link_context: AccountAddress,
        err: Option<&VMError>,
    ) {
        if let Some(err) = err {
            self.trace
                .effect(EF::ExecutionError(format!("{:?}", err.major_status())));
            return;
        }
        let opt = self.close_frame_(
            frame,
            function,
            interpreter,
            loader,
            remaining_gas,
            link_context,
        );
        self.emit_trace_error_if_err(opt.is_none())
    }

    pub(crate) fn open_instruction(
        &mut self,
        frame: &Frame,
        interpreter: &Interpreter,
        loader: &Loader,
        remaining_gas: u64,
    ) {
        let opt = self.open_instruction_(frame, interpreter, loader, remaining_gas);
        self.emit_trace_error_if_err(opt.is_none());
    }

    pub(crate) fn close_instruction(
        &mut self,
        frame: &Frame,
        interpreter: &Interpreter,
        loader: &Loader,
        remaining_gas: u64,
        err: Option<&PartialVMError>,
    ) {
        if self
            .close_instruction_(frame, interpreter, loader, remaining_gas)
            .is_none()
        {
            // If we fail to close the instruction, we need to emit an error event.
            // This can be the case where the instruction itself failed -- e.g. with a division by
            // zero, invalid cast, etc.
            let error_string = match err {
                Some(err) => format!("{:?}", err.major_status()),
                None => "VM tracer failed to close instruction but interpreter was OK -- this is most likely a bug in the tracer".to_string(),
            };
            let pc = self
                .pc
                .expect("PC always set by this point by `open_instruction`");
            let instruction = &frame.function.code()[pc as usize];
            let effects = self.register_post_effects(vec![EF::ExecutionError(error_string)]);
            // TODO: type params here?
            self.trace
                .instruction(instruction, vec![], effects, remaining_gas, pc);
        } else if let Some(err) = err {
            self.trace
                .effect(EF::ExecutionError(format!("{:?}", err.major_status())));
        }
    }
}

impl FunctionTypeInfo {
    /// Resolve a function to all of its type information (type arguments, local types, and return
    /// types).
    fn new(
        function: &Function,
        loader: &Loader,
        ty_args: &[Type],
        link_context: AccountAddress,
    ) -> Option<FunctionTypeInfo> {
        // Split a `Type` into its inner type and reference type.
        let deref_ty = |ty: Type| -> (Type, Option<RefType>) {
            match ty {
                Type::Reference(r) => (*r, Some(RefType::Imm)),
                Type::MutableReference(t) => (*t, Some(RefType::Mut)),
                Type::TyParam(_) => unreachable!("Type parameters should be fully substituted"),
                _ => (ty, None),
            }
        };

        let (module, _) = loader.get_module(link_context, function.module_id());
        let fdef = module.function_def_at(function.index());
        let f_handle = module.function_handle_at(fdef.function);
        let get_types_for_sig = |si: SignatureIndex| -> Option<Vec<TagWithLayoutInfoOpt>> {
            let signatures = &module.signature_at(si).0;
            signatures
                .iter()
                .map(|tok| {
                    let ty = loader.make_type(&module, tok).ok()?;
                    let subst_ty = loader.subst(&ty, ty_args).ok()?;
                    let (ty, ref_type) = deref_ty(subst_ty);
                    let tag = loader.type_to_type_tag(&ty).ok()?;
                    // NB: This may fail if the type represents a value greater than the max
                    // value depth.
                    let type_layout = loader.type_to_fully_annotated_layout(&ty).ok();
                    let layout = (type_layout, ref_type);
                    Some(TagWithLayoutInfoOpt { tag, layout })
                })
                .collect::<Option<Vec<_>>>()
        };
        let mut local_types = get_types_for_sig(f_handle.parameters)?;

        if let Some(code) = fdef.code.as_ref() {
            local_types.extend(get_types_for_sig(code.locals)?);
        }

        let return_types = {
            let signatures = &module.signature_at(f_handle.return_).0;
            signatures
                .iter()
                .map(|tok| {
                    let ty = loader.make_type(&module, tok).ok()?;
                    let subst_ty = loader.subst(&ty, ty_args).ok()?;
                    let (ty, ref_type) = deref_ty(subst_ty);
                    let tag = loader.type_to_type_tag(&ty).ok()?;
                    let type_layout = loader.type_to_fully_annotated_layout(&ty).ok();
                    let layout = (type_layout, ref_type);
                    Some(TagWithLayoutInfoOpt { tag, layout })
                })
                .collect::<Option<Vec<_>>>()?
        };

        let ty_args = ty_args
            .iter()
            .cloned()
            .map(|ty| {
                let (ty, ref_type) = deref_ty(ty);
                assert!(ref_type.is_none());
                loader.type_to_type_tag(&ty).ok()
            })
            .collect::<Option<_>>()?;

        Some(FunctionTypeInfo {
            ty_args,
            local_types,
            return_types,
        })
    }
}

/// Get the type layout of a constant.
fn get_constant_type_layout(
    function: &Function,
    loader: &Loader,
    link_context: AccountAddress,
    const_idx: ConstantPoolIndex,
) -> Option<MoveTypeLayout> {
    let (module, _loaded_module) = loader.get_module(link_context, function.module_id());
    let constant = module.constant_at(const_idx);
    let ty = loader.make_type(&module, &constant.type_).ok()?;
    loader.type_to_fully_annotated_layout(&ty).ok()
}
