// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// IDEA: Post trace analysis -- report when values are dropped.

use crate::interface::{NopTracer, Tracer, Writer};
use move_binary_format::{
    file_format::{Bytecode, FunctionDefinitionIndex as BinaryFunctionDefinitionIndex},
    file_format_common::instruction_opcode,
};
use move_core_types::{
    annotated_value::MoveValue,
    language_storage::{ModuleId, TypeTag},
};
use serde::Serialize;
use std::fmt::Display;

/// An index into the trace. This should be used when referring to locations in the trace.
/// Otherwise, a `usize` should be used when referring to indices that are not in the trace.
pub type TraceIndex = usize;
pub type TraceVersion = u64;

/// The current version of the trace format.
const TRACE_VERSION: TraceVersion = 1;

/// A Location is a valid root for a reference. This can either be a local in a frame, a stack
/// value, or a reference into another location (e.g., vec[0][2]).
///
/// Note that we track aliasing through the locations so you can always trace back to the root
/// value for the reference.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub enum Location {
    // Local index in a frame. The frame is identified by the index in the trace where it was created.
    // The `usize` is the index into the locals of the frame.
    Local(TraceIndex, usize),
    // An indexed location. This is a reference into another location (e.g., due to a borrow field,
    // or a reference into a vector).
    Indexed(Box<Location>, usize),
    // A global reference.
    // Identified by the location in the trace where it was introduced.
    Global(TraceIndex),
}

/// A Read event. This represents a read from a location, with the value read and whether the value
/// was moved or not.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Read {
    pub location: Location,
    pub root_value_read: TraceValue,
    pub moved: bool,
}

/// A Write event. This represents a write to a location with the value written and a snapshot of
/// the value that was written. Note that the `root_value_after_write` is a snapshot of the
/// _entire_ (root) value that was written after the write.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Write {
    pub location: Location,
    pub root_value_after_write: TraceValue,
}

/// A TraceValue is a value in the standard MoveValue domain + references.
/// References hold their own snapshot of the root value they point to, along with the rooted path to
/// the value that they reference within that snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum TraceValue {
    RuntimeValue {
        value: MoveValue,
    },
    ImmRef {
        location: Location,
        // Snapshot of the root value.
        snapshot: Box<MoveValue>,
    },
    MutRef {
        location: Location,
        // Snapshot of the root value.
        snapshot: Box<MoveValue>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum RefType {
    Imm,
    Mut,
}

/// Type tag with references. This is a type tag that also supports references.
/// if ref_type is None, this is a value type. If ref_type is Some, this is a reference type of the
/// given reference type.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TypeTagWithRefs {
    pub type_: TypeTag,
    pub ref_type: Option<RefType>,
}

/// A `Frame` represents a stack frame in the Move VM and a given instantiation of a function.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Frame {
    // The frame id is the offset in the trace where this frame was opened.
    pub frame_id: TraceIndex,
    pub function_name: String,
    pub module: ModuleId,
    // External pointer out into the module -- the `FunctionDefinitionIndex` in the module.
    pub binary_member_index: u16,
    pub type_instantiation: Vec<TypeTag>,
    pub parameters: Vec<TraceValue>,
    pub return_types: Vec<TypeTagWithRefs>,
    pub locals_types: Vec<TypeTagWithRefs>,
    pub is_native: bool,
}

/// An instruction effect is a single effect of an instruction. This can be a push/pop of a value
/// or a reference to a value, or a read/write of a value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum Effect {
    // Pop a value off the stack (pre-effect only)
    Pop(TraceValue),
    // Read a value from a location (pre-effect only)
    Read(Read),

    // Push a value on the stack (post-effect only)
    Push(TraceValue),
    // Write a value to a location (post-effect only)
    Write(Write),

    // A data load Effect
    DataLoad(DataLoad),

    // An execution error occured
    ExecutionError(String),
}

/// Represent a data load event. This is a load of a value from storage. We only record loads by
/// reference in the trace, and we snapshot the value at the reference location at the time of load
/// and record its global reference ID (i.e., the location in the trace at which it was loaded).
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DataLoad {
    pub ref_type: RefType,
    pub location: Location,
    pub snapshot: MoveValue,
}

/// A TraceEvent is a single event in the Move VM, external events can also be interleaved in the
/// trace. MoveVM events, are well structured, and can be a frame event or an instruction event.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub enum TraceEvent {
    OpenFrame {
        frame: Box<Frame>,
        gas_left: u64,
    },
    CloseFrame {
        frame_id: TraceIndex,
        return_: Vec<TraceValue>,
        gas_left: u64,
    },
    Instruction {
        type_parameters: Vec<TypeTag>,
        pc: u16,
        gas_left: u64,
        instruction: Box<String>,
    },
    Effect(Box<Effect>),
    External(Box<serde_json::Value>),
}

#[derive(Serialize, Debug, Clone, Eq, PartialEq)]
pub struct MoveTrace {
    pub version: TraceVersion,
    pub events: Vec<TraceEvent>,
}

/// The Move trace format. The custom tracer is not serialized, but the events are.
/// This is the format that the Move VM will output traces in, and the `tracer` can output
/// additional events to the trace.
pub struct MoveTraceBuilder {
    pub tracer: Box<dyn Tracer>,

    pub trace: MoveTrace,
}

impl TraceValue {
    pub fn snapshot(&self) -> &MoveValue {
        match self {
            TraceValue::ImmRef { snapshot, .. } | TraceValue::MutRef { snapshot, .. } => snapshot,
            TraceValue::RuntimeValue { value } => value,
        }
    }

    pub fn value_mut(&mut self) -> Option<&mut MoveValue> {
        match self {
            TraceValue::RuntimeValue { value, .. } => Some(value),
            _ => None,
        }
    }

    pub fn location(&self) -> Option<&Location> {
        match self {
            TraceValue::ImmRef { location, .. } | TraceValue::MutRef { location, .. } => {
                Some(location)
            }
            _ => None,
        }
    }
}

impl MoveTrace {
    pub fn new() -> Self {
        Self {
            version: TRACE_VERSION,
            events: vec![],
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self).unwrap()
    }
}

impl Default for MoveTrace {
    fn default() -> Self {
        Self::new()
    }
}

impl MoveTraceBuilder {
    /// Create a new `MoveTraceBuilder` with no additional tracing.
    pub fn new() -> Self {
        Self {
            tracer: Box::new(NopTracer),
            trace: MoveTrace::new(),
        }
    }

    /// Create a new `MoveTraceBuilder` with a custom `tracer`.
    pub fn new_with_tracer(tracer: Box<dyn Tracer>) -> Self {
        Self {
            tracer,
            trace: MoveTrace::new(),
        }
    }

    /// Consume the `MoveTraceBuilder` and return the `MoveTrace` that has been built by it.
    pub fn into_trace(self) -> MoveTrace {
        self.trace
    }

    /// Get the current offset in the `MoveTrace` that is being built.
    pub fn current_trace_offset(&self) -> TraceIndex {
        self.trace.events.len()
    }

    /// Record an `OpenFrame` event in the trace.
    pub fn open_frame(
        &mut self,
        frame_id: TraceIndex,
        binary_member_index: BinaryFunctionDefinitionIndex,
        name: String,
        module: ModuleId,
        parameters: Vec<TraceValue>,
        type_instantiation: Vec<TypeTag>,
        return_types: Vec<TypeTagWithRefs>,
        locals_types: Vec<TypeTagWithRefs>,
        is_native: bool,
        gas_left: u64,
    ) {
        let frame = Box::new(Frame {
            frame_id,
            function_name: name,
            module,
            binary_member_index: binary_member_index.0,
            type_instantiation,
            parameters,
            return_types,
            locals_types,
            is_native,
        });
        self.push_event(TraceEvent::OpenFrame { frame, gas_left });
    }

    /// Record a `CloseFrame` event in the trace.
    pub fn close_frame(&mut self, frame_id: TraceIndex, return_: Vec<TraceValue>, gas_left: u64) {
        self.push_event(TraceEvent::CloseFrame {
            frame_id,
            return_,
            gas_left,
        });
    }

    /// Record an `Instruction` event in the trace along with the effects of the instruction.
    pub fn instruction(
        &mut self,
        instruction: &Bytecode,
        type_parameters: Vec<TypeTag>,
        effects: Vec<Effect>,
        gas_left: u64,
        pc: u16,
    ) {
        self.push_event(TraceEvent::Instruction {
            type_parameters,
            pc,
            gas_left,
            instruction: Box::new(format!("{:?}", instruction_opcode(instruction))),
        });
        for effect in effects {
            self.push_event(TraceEvent::Effect(Box::new(effect)));
        }
    }

    /// Push an `Effect` event to the trace.
    pub fn effect(&mut self, effect: Effect) {
        self.push_event(TraceEvent::Effect(Box::new(effect)));
    }

    // All events pushed to the trace are first pushed, and then the tracer is notified of the
    // event.
    fn push_event(&mut self, event: TraceEvent) {
        self.trace.events.push(event.clone());
        self.tracer.notify(&event, Writer(&mut self.trace));
    }
}

impl Default for MoveTraceBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl Display for TraceValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceValue::RuntimeValue { value } => {
                write!(f, "{:#}", value)
            }
            TraceValue::ImmRef { location, snapshot } => {
                write!(f, "&{} {:#}", location, snapshot)
            }
            TraceValue::MutRef { location, snapshot } => {
                write!(f, "&mut {} {:#}", location, snapshot)
            }
        }
    }
}

impl Display for Location {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Location::Local(frame_idx, idx) => {
                write!(f, "l{idx}@{frame_idx}")
            }
            Location::Indexed(loc, offset) => {
                write!(f, "{loc}[{offset}]")
            }
            Location::Global(id) => {
                write!(f, "g{}", id)
            }
        }
    }
}

impl Display for Effect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Effect::Pop(value) => {
                write!(f, "Pop {}", value)
            }
            Effect::Push(value) => {
                write!(f, "Push {}", value)
            }
            Effect::Read(Read {
                location,
                root_value_read: value_read,
                moved,
            }) => {
                let arrow = if *moved { "==>" } else { "-->" };
                write!(f, "{location} {arrow} {value_read}")
            }
            Effect::Write(Write {
                location,
                root_value_after_write: value_written,
            }) => {
                write!(f, "{location} <-- {value_written}")
            }
            Effect::ExecutionError(error_string) => {
                write!(f, "ExecutionError: {error_string}")
            }
            Effect::DataLoad(DataLoad {
                ref_type,
                location,
                snapshot,
            }) => {
                let ref_type = match ref_type {
                    RefType::Imm => "&",
                    RefType::Mut => "&mut",
                };
                write!(f, "g{ref_type}{location} ~~> {snapshot}")
            }
        }
    }
}
