// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::fmt::Display;

use move_binary_format::{
    file_format::{Bytecode, FunctionDefinitionIndex as BinaryFunctionDefinitionIndex},
    file_format_common::instruction_opcode,
};
use move_core_types::{
    annotated_value::MoveValue,
    language_storage::{ModuleId, TypeTag},
};
use serde::Serialize;

use crate::trace_interface::{NopTracer, Tracer, Writer};

pub type FrameIdentifier = usize;
pub type TraceLocalUID = u64;
pub type TraceVersion = u64;

const TRACE_VERSION: TraceVersion = 0;

/// A Location is a valid root for a reference. This can either be a local in a frame, a stack
/// value, or a reference into another location (e.g., vec[0][2]).
///
/// Note that we track aliasing through the locations so you can always trace back to the root
/// value for the reference.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub enum Location {
    // Local index in a frame
    Local(FrameIdentifier, usize),
    // value id , offset into value at that index
    Indexed(Box<Location>, usize),
    // A global reference
    Global(TraceLocalUID),
}

/// A Read event. This represents a read from a location, with the value read and whether the value
/// was moved or not.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Read {
    pub location: Location,
    pub value_read: TraceValue,
    pub moved: bool,
}

// A Write event. This represents a write to a location with the value written and a snapshot of
// the value that was written.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Write {
    pub location: Location,
    pub value_written: TraceValue,
}

/// A TraceValue is a value is the standard MoveValue domain + references.
/// References hold their own snapshot of the value they point to, along with the rooted path to
/// the value that they reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum TraceValue {
    RuntimeValue {
        value: MoveValue,
    },
    ImmRef {
        location: Location,
        snapshot: Box<MoveValue>,
    },
    MutRef {
        location: Location,
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
    pub layout: TypeTag,
    pub ref_type: Option<RefType>,
}

/// A Frame represents a stack frame in the Move VM.
/// This is an instantiation of `Function`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Frame {
    pub frame_id: FrameIdentifier,
    pub function_name: String,
    pub module: ModuleId,
    // External pointer out into the module
    pub binary_member_index: u16,
    pub type_instantiation: Vec<TypeTag>,
    pub parameters: Vec<TraceValue>,
    pub return_types: Vec<TypeTagWithRefs>,
    pub locals_types: Vec<TypeTagWithRefs>,
    pub is_native: bool,
}

/// An instruction effect is a single effect of an instruction. This can be a push/pop of a value,
/// a read/write of a value, or a reference to a value.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum Effect {
    // Pop a value off of the stack (pre-effect only)
    Pop(TraceValue),
    // Read a value from a location (pre-effect only)
    Read(Read),

    // Push a value off the stack (post-effect only)
    Push(TraceValue),
    // Write a value to a location (post-effect only)
    Write(Write),

    // A data load Effect
    DataLoad(DataLoad),

    // An execution error occured
    ExecutionError,
}

/// An instruction event is a single instruction in the Move VM. This includes the type parameters
/// that it was executed with, the effects of the instruction, and the program counter that the
/// instruction encountered at.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct InstructionEvent {
    pub type_parameters: Vec<TypeTag>,
    pub effects: Vec<Effect>,
    pub pc: u16,
    pub gas_left: u64,
    pub instruction: String,
}

/// A frame event is the beginning (open) or end (close) of a frame in the Move VM.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum FrameEvent {
    Open {
        frame: Frame,
        gas_left: u64,
    },
    Close {
        frame_id: FrameIdentifier,
        return_: Vec<TraceValue>,
        gas_left: u64,
    },
}

/// Represent a data load event. This is a load of a value from storage. Either by value in which
/// case we simply snapshot the value at that point (it will be subsequently moved onto the stack
/// or into a local), or by reference in which case we snapshot the value at the reference
/// location at the time of load and record its global reference ID.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum DataLoad {
    ByValue(MoveValue),
    ByReference {
        ref_type: RefType,
        location: Location,
        snapshot: MoveValue,
    },
}

/// A TraceEvent is a single event in the Move VM, external events can also be interleaved in the
/// trace. MoveVM events, are well structured, and can be a frame event or an instruction event.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub enum TraceEvent {
    OpenFrame {
        frame: Frame,
        gas_left: u64,
    },
    CloseFrame {
        frame_id: FrameIdentifier,
        return_: Vec<TraceValue>,
        gas_left: u64,
    },
    Instruction {
        type_parameters: Vec<TypeTag>,
        pc: u16,
        gas_left: u64,
        instruction: String,
    },
    Effect(Effect),
    External(serde_json::Value),
}

/// The Move trace format. The custom tracer is not serialized, but the events are.
/// This is the format that the Move VM will output traces in, and the `tracer` can output
/// additiional events to the trace.
pub struct MoveTraceBuilder {
    pub tracer: Box<dyn Tracer>,
    pub uid_counter: u64,

    pub trace: MoveTrace,
}

#[derive(Serialize)]
pub struct MoveTrace {
    pub version: TraceVersion,
    pub events: Vec<TraceEvent>,
}

impl TraceValue {
    pub fn snapshot(&self) -> &MoveValue {
        match self {
            TraceValue::ImmRef { snapshot, .. } | TraceValue::MutRef { snapshot, .. } => {
                &**snapshot
            }
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

impl MoveTraceBuilder {
    pub fn new() -> Self {
        Self {
            tracer: Box::new(NopTracer),
            uid_counter: 0,
            trace: MoveTrace::new(),
        }
    }

    pub fn new_with_tracer(tracer: Box<dyn Tracer>) -> Self {
        Self {
            tracer,
            uid_counter: 0,
            trace: MoveTrace::new(),
        }
    }

    pub fn set_tracer(&mut self, tracer: Box<dyn Tracer>) {
        self.tracer = tracer;
    }

    pub fn into_trace(self) -> MoveTrace {
        self.trace
    }

    pub fn new_trace_local_uid(&mut self) -> TraceLocalUID {
        let uid = self.uid_counter;
        self.uid_counter += 1;
        uid
    }

    pub fn open_frame(
        &mut self,
        frame_id: FrameIdentifier,
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
        let frame = Frame {
            frame_id,
            function_name: name,
            module,
            binary_member_index: binary_member_index.0 as u16,
            type_instantiation,
            parameters,
            return_types,
            locals_types,
            is_native,
        };
        self.push_event(TraceEvent::OpenFrame { frame, gas_left });
    }

    pub fn close_frame(
        &mut self,
        frame_id: FrameIdentifier,
        return_: Vec<TraceValue>,
        gas_left: u64,
    ) {
        self.push_event(TraceEvent::CloseFrame {
            frame_id,
            return_,
            gas_left,
        });
    }

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
            instruction: format!("{:?}", instruction_opcode(instruction)),
        });
        for effect in effects {
            self.push_event(TraceEvent::Effect(effect));
        }
    }

    pub fn effect(&mut self, effect: Effect) {
        self.push_event(TraceEvent::Effect(effect));
    }

    fn push_event(&mut self, event: TraceEvent) {
        self.trace.events.push(event.clone());
        self.tracer.notify(&event, Writer(&mut self.trace));
    }
}

impl Display for TraceValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceValue::RuntimeValue { value } => {
                write!(f, "{:#}", value)
            }
            TraceValue::ImmRef { location, snapshot } => {
                write!(f, "&({}) {}", location, snapshot)
            }
            TraceValue::MutRef { location, snapshot } => {
                write!(f, "&mut({}) {}", location, snapshot)
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
                write!(f, "({loc})[{offset}]")
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
                value_read,
                moved,
            }) => {
                let arrow = if *moved { "==>" } else { "-->" };
                write!(f, "{location} {arrow} {value_read}")
            }
            Effect::Write(Write {
                location,
                value_written,
            }) => {
                write!(f, "{location} <- {value_written}")
            }
            Effect::ExecutionError => {
                write!(f, "ExecutionError")
            }
            Effect::DataLoad(data_load) => match data_load {
                DataLoad::ByValue(value) => {
                    write!(f, "G ~~> {:#}", value)
                }
                DataLoad::ByReference {
                    ref_type,
                    location,
                    snapshot,
                } => {
                    let ref_type = match ref_type {
                        RefType::Imm => "&",
                        RefType::Mut => "&mut",
                    };
                    write!(f, "G{ref_type}{location} ~~> {snapshot}")
                }
            },
        }
    }
}
