use std::{collections::BTreeMap, fmt::Display};

use move_binary_format::file_format::FunctionDefinitionIndex as BinaryFunctionDefinitionIndex;
use move_core_types::{
    annotated_value::MoveValue,
    language_storage::{ModuleId, TypeTag},
};
use serde::Serialize;

pub type FrameIdentifier = usize;

/// A Location is a valid root for a reference. This can either be a local in a frame, a stack
/// value, or a reference into another location (e.g., vec[0][2]).
///
/// Note that we track aliasing through the locations so you can always trace back to the root
/// value for the reference.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub enum Location {
    // Local index in a frame
    Local(FrameIdentifier, usize),
    Stack(usize),
    // value id , offset into value at that index
    Indexed(Box<Location>, usize),
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

/// A ReferenceableValue is a value is the standard MoveValue domain + references.
/// References hold their own snapshot of the value they point to, along with the rooted path to
/// the value that they reference.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum TraceValue {
    Value {
        value: MoveValue,
    },
    ImmRef {
        location: Location,
        snapshot: Box<TraceValue>,
    },
    MutRef {
        location: Location,
        snapshot: Box<TraceValue>,
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
#[derive(Debug, Eq, PartialEq, Serialize)]
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
}

/// An instruction effect is a single effect of an instruction. This can be a push/pop of a value,
/// a read/write of a value, or a reference to a value.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub enum InstructionEffect {
    // Pop a value off of the stack (pre-effect only)
    Pop(TraceValue),
    // Read a value from a location (pre-effect only)
    Read(Read),

    // Push a value off the stack (post-effect only)
    Push(TraceValue),
    // Write a value to a location (post-effect only)
    Write(Write),
}

/// An instruction event is a single instruction in the Move VM. This includes the type parameters
/// that it was executed with, the effects of the instruction, and the program counter that the
/// instruction encountered at.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct InstructionEvent {
    pub type_parameters: Vec<TypeTag>,
    pub effects: Vec<InstructionEffect>,
    pub pc: u16,
}

/// A frame event is the beginning (open) or end (close) of a frame in the Move VM.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub enum FrameEvent {
    Open {
        frame: Frame,
    },
    Close {
        frame_id: FrameIdentifier,
        return_: Vec<TraceValue>,
    },
}

/// A TraceEvent is a single event in the Move VM. This can be a frame event or an instruction
/// event.
#[derive(Debug, Eq, PartialEq, Serialize)]
pub enum TraceEvent_ {
    Frame(FrameEvent),
    Instruction(InstructionEvent),
}

/// Every trace event has a gas left. We can also add other metadata here later on if we want (e.g., timings etc)
#[derive(Debug, Eq, PartialEq, Serialize)]
pub struct TraceEvent {
    pub event: TraceEvent_,
    pub gas_left: u64,
}

/// The Move trace format
#[derive(Debug, Serialize)]
pub struct MoveTrace {
    pub trace: Vec<TraceEvent>,
}

impl TraceValue {
    pub fn value(&self) -> Option<&MoveValue> {
        match self {
            TraceValue::Value { value, .. } => Some(value),
            _ => None,
        }
    }

    pub fn value_mut(&mut self) -> Option<&mut MoveValue> {
        match self {
            TraceValue::Value { value, .. } => Some(value),
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
        Self { trace: vec![] }
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
        };
        self.trace.push(TraceEvent {
            event: TraceEvent_::Frame(FrameEvent::Open { frame }),
            gas_left,
        });
    }

    pub fn close_frame(
        &mut self,
        frame_id: FrameIdentifier,
        return_: Vec<TraceValue>,
        gas_left: u64,
    ) {
        self.trace.push(TraceEvent {
            event: TraceEvent_::Frame(FrameEvent::Close { frame_id, return_ }),
            gas_left,
        });
    }

    pub fn instruction(
        &mut self,
        type_parameters: Vec<TypeTag>,
        effects: Vec<InstructionEffect>,
        gas_left: u64,
        pc: u16,
    ) {
        self.trace.push(TraceEvent {
            event: TraceEvent_::Instruction(InstructionEvent {
                type_parameters,
                effects,
                pc,
            }),
            gas_left,
        });
    }
}

impl Display for TraceValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TraceValue::Value { value } => {
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
            Location::Stack(idx) => {
                write!(f, "s@{idx}")
            }
            Location::Indexed(loc, offset) => {
                write!(f, "({loc})[{offset}]")
            }
        }
    }
}

impl MoveTrace {
    pub fn get_mut_location<'a>(
        location: &Location,
        stack: &'a mut Vec<TraceValue>,
        locals: &'a mut BTreeMap<FrameIdentifier, BTreeMap<usize, TraceValue>>,
    ) -> &'a mut MoveValue {
        match location {
            Location::Local(frame_idx, idx) => {
                let frame = locals.get_mut(frame_idx).unwrap();
                frame.get_mut(idx).unwrap().value_mut().unwrap()
            }
            Location::Stack(idx) => stack[*idx].value_mut().unwrap(),
            Location::Indexed(loc, offset) => {
                let val = Self::get_mut_location(loc, stack, locals);
                match val {
                    MoveValue::Vector(v) => v.get_mut(*offset).unwrap(),
                    MoveValue::Struct(s) => &mut s.fields.get_mut(*offset).unwrap().1,
                    _ => unreachable!(),
                }
            }
        }
    }

    pub fn reconstruct(&self) {
        let mut frames: BTreeMap<FrameIdentifier, BTreeMap<usize, TraceValue>> = BTreeMap::new();
        let mut values = vec![];
        for event in &self.trace {
            match &event.event {
                TraceEvent_::Frame(frame_event) => match frame_event {
                    FrameEvent::Open { frame } => {
                        println!("Open frame: {}", frame.function_name);
                        let mut locals = BTreeMap::new();
                        for (i, p) in frame.parameters.iter().enumerate() {
                            values.pop().unwrap();
                            locals.insert(i, p.clone());
                        }
                        frames.insert(frame.frame_id, locals);
                    }
                    FrameEvent::Close {
                        frame_id,
                        return_: _,
                    } => {
                        frames.remove(frame_id).unwrap();
                        println!("Close frame {frame_id}");
                    }
                },
                TraceEvent_::Instruction(instruction_event) => {
                    println!("Instruction:");
                    for effect in &instruction_event.effects {
                        match effect {
                            InstructionEffect::Push(value) => {
                                println!(
                                    ">\tPush {}",
                                    format!("{:#}", value).replace("\n", "\n\t")
                                );
                                values.push(value.clone());
                            }
                            InstructionEffect::Pop(value) => {
                                println!(">\tPop {}", format!("{:#}", value).replace("\n", "\n\t"));
                                values.pop().unwrap();
                            }
                            InstructionEffect::Read(Read {
                                location,
                                value_read,
                                moved,
                            }) => {
                                if *moved {
                                    match location {
                                        Location::Local(frame_idx, idx) => {
                                            let frame = frames.get_mut(frame_idx).unwrap();
                                            frame.remove(&idx);
                                        }
                                        Location::Indexed(..) => (),
                                        Location::Stack(_) => unreachable!(),
                                    }
                                }
                                println!(
                                    ">\tRead: {} -{moved}-> {}",
                                    location,
                                    format!("{:#}", value_read).replace("\n", "\n\t")
                                );
                            }
                            InstructionEffect::Write(Write {
                                location,
                                value_written,
                            }) => {
                                match location {
                                    Location::Local(frame_idx, idx) => {
                                        let frame = frames.get_mut(frame_idx).unwrap();
                                        frame.insert(*idx, value_written.clone());
                                    }
                                    Location::Indexed(location, idx) => {
                                        let val = Self::get_mut_location(
                                            location,
                                            &mut values,
                                            &mut frames,
                                        );
                                        match val {
                                            MoveValue::Vector(v) => {
                                                v[*idx] =
                                                    value_written.clone().value().unwrap().clone();
                                            }
                                            MoveValue::Struct(s) => {
                                                s.fields[*idx].1 =
                                                    value_written.clone().value().unwrap().clone();
                                            }
                                            _ => unreachable!(),
                                        }
                                    }
                                    Location::Stack(idx) => {
                                        values[*idx] = value_written.clone();
                                    }
                                }
                                println!(
                                    ">\tWrite: {} <- {}",
                                    location,
                                    format!("{:#}", value_written).replace("\n", "\n\t")
                                );
                            }
                        }
                    }
                }
            }
            println!("stack:");
            for val in &values {
                println!("\t{}", format!("{:#}", val).replace("\n", "\n\t"));
            }

            if frames.is_empty() {
                continue;
            }
            println!("locals:");
            for (frame_id, frame) in &frames {
                if frame.is_empty() {
                    continue;
                }
                println!("\tFrame {}", frame_id);
                for (i, val) in frame {
                    println!("\t\t{i}: {}", format!("{:#}", val).replace("\n", "\n\t\t"));
                }
            }
        }
    }
}
