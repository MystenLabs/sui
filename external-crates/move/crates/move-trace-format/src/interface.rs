// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::format::{MoveTrace, TraceEvent, TraceVersion};
use move_binary_format::file_format_common::Opcodes;
use serde::Serialize;

/// This is meant to be an internal tracing interface for the VM, and should only be implemented
/// and used if you are _sure_ that you want/need to use it. Generally you should use the output
/// trace format for any analysis or debugging purposes. This should only be used if you want to
/// add custom tracing data to the VM's traces that cannot be added using other means or
/// post-processing.
pub trait Tracer {
    /// Notify the tracer of a new event in the VM. This is called for every event that is emitted,
    /// and immediatlye _after_ the `event` has been added to the trace held inside of the `writer`.
    fn notify(&mut self, event: &TraceEvent, writer: Writer<'_>) -> bool;

    /// Whether this tracer wants effect events for an instruction (and possibly only a specific
    /// subset of those events). If the events are not needed the VM tracer can skip the expensive
    /// value conversion work needed to build effects.
    fn instruction_filter(&self, instruction: &Opcodes, pc: u16) -> Option<EventFilter>;
}

impl<T: Tracer> Tracer for &mut T {
    fn notify(&mut self, event: &TraceEvent, writer: Writer<'_>) -> bool {
        <T as Tracer>::notify(self, event, writer)
    }

    fn instruction_filter(&self, instruction: &Opcodes, pc: u16) -> Option<EventFilter> {
        <T as Tracer>::instruction_filter(self, instruction, pc)
    }
}

/// A writer that allows you to push custom events to the trace but encapsulates the trace so that
/// non-external events cannot be accidentally added.
pub struct Writer<'a>(pub(crate) &'a mut MoveTrace);

impl Writer<'_> {
    /// Emit an external event into the trace.
    pub fn push<T: Serialize>(&mut self, e: T) {
        self.0.push_event(TraceEvent::External(Box::new(
            serde_json::to_value(e).unwrap(),
        )));
    }

    /// Get the current version of the trace.
    pub fn trace_version(&mut self) -> TraceVersion {
        self.0.version
    }
}

// NB: event indices are 1-based so that there is a clear distinction betwen pre-instruction
// events (negative indices) and post-instruction events (positive indices).
// pre effects - negative indices
// post effects - positive indices
// e.g., For Add
// -2     -1     1
// Pop    Pop    Push
// So if you were interested in the post effects of an `Add` instruction, you would want to filter
// for event indices > 0. If you were interested in the pre effects of a `Sub` instruction, you would
// want to filter for event indices < 0.
// fn instruction_filter_example(instruction: &Opcodes, pc: u16) -> Option<EventFilter> {
//     match instruction {
//         // Only interested in the post effects of an `Add` instruction
//         Opcodes::ADD => Some(|relative_event_index: i16| relative_event_index > 0),
//         // Only interested in the pre effects of a `Sub` instruction
//         Opcodes::SUB => Some(|relative_event_index: i16| relative_event_index < 0),
//         // Otherwise, we don't care about the effects of this instruction
//         _ => None,
//     }
// }
pub type EventFilter = fn(event_index: i16) -> bool;
