// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// IDEA: Post trace analysis -- report when values are dropped.

use crate::interface::{NopTracer, Tracer, Writer};
use crate::value::SerializableMoveValue;
use move_binary_format::{
    file_format::{Bytecode, FunctionDefinitionIndex as BinaryFunctionDefinitionIndex},
    file_format_common::instruction_opcode,
};
use move_core_types::{
    account_address::AccountAddress,
    language_storage::{ModuleId, TypeTag},
};
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader};
use std::{fmt::Display, sync::mpsc::Receiver};

/// An index into the trace. This should be used when referring to locations in the trace.
/// Otherwise, a `usize` should be used when referring to indices that are not in the trace.
pub type TraceIndex = usize;
pub type TraceVersion = u64;

/// json.zst file extension for the trace since that is what we compress with
pub const TRACE_FILE_EXTENSION: &str = "json.zst";

/// The current version of the trace format.
const TRACE_VERSION: TraceVersion = 3;

/// Compression level for the trace. This is the level of compression that we will use for the
/// trace in zstd.
const COMPRESSION_LEVEL: i32 = 1;

/// Size of the compression chunk. This is the size of the buffer that we will compress at a time.
const COMPRESSION_CHUNK_SIZE: usize = 1024 * 1024 * 1024;

/// Size of the channel buffer. This is the size of the buffer that we will use to buffer events
/// before adding backpressure to the tracer.
const CHANNEL_BUFFER_SIZE: usize = 100;

/// A Location is a valid root for a reference. This can either be a local in a frame, a stack
/// value, or a reference into another location (e.g., vec[0][2]).
///
/// Note that we track aliasing through the locations so you can always trace back to the root
/// value for the reference.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Read {
    pub location: Location,
    pub root_value_read: TraceValue,
    pub moved: bool,
}

/// A Write event. This represents a write to a location with the value written and a snapshot of
/// the value that was written. Note that the `root_value_after_write` is a snapshot of the
/// _entire_ (root) value that was written after the write.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Write {
    pub location: Location,
    pub root_value_after_write: TraceValue,
}

/// A TraceValue is a value in the standard MoveValue domain + references.
/// References hold their own snapshot of the root value they point to, along with the rooted path to
/// the value that they reference within that snapshot.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TraceValue {
    RuntimeValue {
        value: SerializableMoveValue,
    },
    ImmRef {
        location: Location,
        // Snapshot of the root value.
        snapshot: Box<SerializableMoveValue>,
    },
    MutRef {
        location: Location,
        // Snapshot of the root value.
        snapshot: Box<SerializableMoveValue>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RefType {
    Imm,
    Mut,
}

/// Type tag with references. This is a type tag that also supports references.
/// if ref_type is None, this is a value type. If ref_type is Some, this is a reference type of the
/// given reference type.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TypeTagWithRefs {
    pub type_: TypeTag,
    pub ref_type: Option<RefType>,
}

/// A `Frame` represents a stack frame in the Move VM and a given instantiation of a function.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    // The frame id is the offset in the trace where this frame was opened.
    pub frame_id: TraceIndex,
    pub function_name: String,
    pub module: ModuleId,
    pub version_id: AccountAddress,
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
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataLoad {
    pub ref_type: RefType,
    pub location: Location,
    pub snapshot: SerializableMoveValue,
}

/// A TraceEvent is a single event in the Move VM, external events can also be interleaved in the
/// trace. MoveVM events, are well structured, and can be a frame event or an instruction event.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TraceVersionData {
    version: TraceVersion,
}

pub struct BufferedEventStream {
    pub event_count: TraceIndex,
    handle: std::thread::JoinHandle<Vec<u8>>,
    sender: std::sync::mpsc::SyncSender<TraceEvent>,
}

pub struct MoveTrace {
    pub version: TraceVersion,
    buf: BufferedEventStream,
}

/// The Move trace format. The custom tracer is not serialized, but the events are.
/// This is the format that the Move VM will output traces in, and the `tracer` can output
/// additional events to the trace.
pub struct MoveTraceBuilder {
    pub tracer: Box<dyn Tracer>,

    pub trace: MoveTrace,
}

impl TraceValue {
    pub fn snapshot(&self) -> &SerializableMoveValue {
        match self {
            TraceValue::ImmRef { snapshot, .. } | TraceValue::MutRef { snapshot, .. } => snapshot,
            TraceValue::RuntimeValue { value } => value,
        }
    }

    pub fn value_mut(&mut self) -> Option<&mut SerializableMoveValue> {
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

impl BufferedEventStream {
    pub fn new() -> Self {
        let (tx, rx): (_, Receiver<TraceEvent>) =
            std::sync::mpsc::sync_channel(CHANNEL_BUFFER_SIZE);
        let handle = std::thread::spawn(move || {
            use std::io::Write;
            let mut events = zstd::stream::Encoder::new(Vec::new(), COMPRESSION_LEVEL).unwrap();
            serde_json::to_writer(
                &mut events,
                &TraceVersionData {
                    version: TRACE_VERSION,
                },
            )
            .unwrap();
            writeln!(&mut events).unwrap();
            let mut buf = Vec::new();
            for event in rx {
                serde_json::to_writer(&mut buf, &event).unwrap();
                writeln!(&mut buf).unwrap();

                if buf.len() > COMPRESSION_CHUNK_SIZE {
                    events.write_all(&std::mem::take(&mut buf)).unwrap();
                }
            }

            events.write_all(buf.as_slice()).unwrap();
            events.finish().unwrap()
        });

        Self {
            event_count: 0,
            handle,
            sender: tx,
        }
    }

    pub fn push(&mut self, event: TraceEvent) {
        self.sender.send(event).unwrap();
        self.event_count += 1;
    }

    pub fn finish(self) -> Vec<u8> {
        // close channel
        drop(self.sender);
        self.handle.join().unwrap()
    }
}

impl MoveTrace {
    pub fn new() -> Self {
        Self {
            version: TRACE_VERSION,
            buf: BufferedEventStream::new(),
        }
    }

    pub fn push_event(&mut self, event: TraceEvent) {
        self.buf.push(event);
    }

    pub fn into_compressed_json_bytes(self) -> Vec<u8> {
        self.buf.finish()
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
        self.trace.buf.event_count
    }

    /// Record an `OpenFrame` event in the trace.
    pub fn open_frame(
        &mut self,
        frame_id: TraceIndex,
        binary_member_index: BinaryFunctionDefinitionIndex,
        name: String,
        module: ModuleId,
        version_id: AccountAddress,
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
            version_id,
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
    pub fn push_event(&mut self, event: TraceEvent) {
        self.trace.push_event(event.clone());
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

// Streaming reader for Move traces

pub struct MoveTraceReader<'a, R: std::io::Read> {
    pub version: TraceVersion,
    reader: BufReader<zstd::stream::Decoder<'a, BufReader<R>>>,
}

impl<R: std::io::Read> MoveTraceReader<'_, R> {
    pub fn new(data: R) -> std::io::Result<Self> {
        let data = zstd::stream::Decoder::new(data)?;
        let mut reader = std::io::BufReader::new(data);
        let mut buf = String::new();
        reader.read_line(&mut buf)?;
        let version: TraceVersionData = serde_json::from_str(&buf)?;
        Ok(Self {
            version: version.version,
            reader,
        })
    }

    pub fn next_event(&mut self) -> std::io::Result<Option<TraceEvent>> {
        let mut buf = String::new();
        match self.reader.read_line(&mut buf) {
            Ok(0) => Ok(None),
            Ok(_) => Ok(Some(serde_json::from_str(&buf)?)),
            Err(e) => Err(e),
        }
    }
}

impl<R: std::io::Read> Iterator for MoveTraceReader<'_, R> {
    type Item = std::io::Result<TraceEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.next_event() {
            Ok(Some(event)) => Some(Ok(event)),
            Ok(None) => None,
            Err(e) => Some(Err(e)),
        }
    }
}

#[test]
fn emit_trace() {
    let mut builder = MoveTraceBuilder::new();
    for i in 0..10 {
        builder.push_event(TraceEvent::External(Box::new(serde_json::json!({
            "event": "external",
            "data": i,
        }))));
    }

    let bytes = builder.into_trace().into_compressed_json_bytes();
    let reader = MoveTraceReader::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(reader.version, TRACE_VERSION);

    for (i, event) in reader.enumerate() {
        let event = event.unwrap();
        let TraceEvent::External(event) = event else {
            panic!("unexpected event: {:?}", event);
        };
        assert_eq!(event.get("data").unwrap().as_u64().unwrap(), i as u64);
    }
}

// Make sure that we can handle large numeric values in the trace (both ser and deser) as in the
// previous value format that we used in the trace we needed to handle large numeric values
// (e.g., u256, u128, u64) that are larger than what serde_json can handle natively with `Number`.
// Since we switched to a typed value format, this is no longer an issue, but we should test to
// make sure that we can still serialize and deserialize these values correctly and prevent any
// possible regressions.
#[test]
fn large_numeric_values_in_trace() {
    use move_core_types::u256;
    let mut builder = MoveTraceBuilder::new();
    let effects = vec![
        Effect::Push(TraceValue::RuntimeValue {
            value: SerializableMoveValue::U256(u256::U256::max_value()),
        }),
        Effect::Push(TraceValue::RuntimeValue {
            value: SerializableMoveValue::U128(u128::MAX),
        }),
        Effect::Push(TraceValue::RuntimeValue {
            value: SerializableMoveValue::U64(u64::MAX),
        }),
    ];

    for eff in effects {
        builder.push_event(TraceEvent::Effect(Box::new(eff)));
    }

    let bytes = builder.into_trace().into_compressed_json_bytes();

    let reader = MoveTraceReader::new(std::io::Cursor::new(bytes)).unwrap();
    assert_eq!(reader.version, TRACE_VERSION);

    for event in reader {
        let event = event.unwrap();
        let TraceEvent::Effect(event) = event else {
            panic!("unexpected event: {:?}", event);
        };

        let Effect::Push(value) = &*event else {
            panic!("expected Push event, got: {:?}", event);
        };

        match value {
            TraceValue::RuntimeValue { value } => match value {
                SerializableMoveValue::U256(v) => assert_eq!(*v, u256::U256::max_value()),
                SerializableMoveValue::U128(v) => assert_eq!(*v, u128::MAX),
                SerializableMoveValue::U64(v) => assert_eq!(*v, u64::MAX),
                _ => panic!("unexpected value type: {:?}", value),
            },
            _ => panic!("expected RuntimeValue, got: {:?}", value),
        }
    }
}
