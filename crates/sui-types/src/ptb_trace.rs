// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_trace_format::{format::TypeTagWithRefs, value::SerializableMoveValue};

use serde::Serialize;

/// PTB-related vents to be stored in the trace. The first one is a summary
/// of the whole PTB, and the following ones represent individual PTB commands.
#[derive(Clone, Debug, Serialize)]
pub enum PTBEvent {
    Summary(SummaryEvent),
    ExternalEvent(ExternalEvent),
    MoveCallStart, // just a marker, all required info is in OpenFrame event
    MoveCallEnd,   // just a marker to make identifying the end of a MoveCall easier
}

#[derive(Clone, Debug, Serialize)]
pub struct SummaryEvent {
    pub name: String,
    pub events: Vec<PTBCommandInfo>,
}

/// Information about the external event that is stored in the trace.
#[derive(Clone, Debug, Serialize)]
pub struct ExternalEvent {
    /// A longer description of the event
    pub description: String,
    /// A shorter name of the event.
    pub name: String,
    /// Values associated with the event.
    pub values: Vec<ExtMoveValue>,
}

/// Information about the PTB commands to be stored in the PTB start event.
/// It contains only the information needed to provide a summary of the command.
#[derive(Clone, Debug, Serialize)]
pub enum PTBCommandInfo {
    MoveCall {
        pkg: String,
        module: String,
        function: String,
    },
    ExternalEvent(String),
}

/// Information about Move value stored in external trace events.
#[derive(Clone, Debug, Serialize)]
pub struct ExtMoveValueInfo {
    pub type_: TypeTagWithRefs,
    pub value: SerializableMoveValue,
}

/// Represents a Move value stored in external trace events.
#[derive(Clone, Debug, Serialize)]
pub enum ExtMoveValue {
    Single {
        name: String,
        info: ExtMoveValueInfo,
    },
    Vector {
        name: String,
        type_: TypeTagWithRefs,
        value: Vec<SerializableMoveValue>,
    },
}
