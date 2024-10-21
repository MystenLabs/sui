// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::format::{MoveTrace, TraceEvent, TraceVersion};
use serde::Serialize;

/// This is meant to be an internal tracing interface for the VM, and should only be implemented
/// and used if you are _sure_ that you want/need to use it. Generally you should use the output
/// trace format for any analysis or debugging purposes. This should only be used if you want to
/// add custom tracing data to the VM's traces that cannot be added using other means or
/// post-processing.
pub trait Tracer {
    /// Notify the tracer of a new event in the VM. This is called for every event that is emitted,
    /// and immediatlye _after_ the `event` has been added to the trace held inside of the `writer`.
    fn notify(&mut self, event: &TraceEvent, writer: Writer<'_>);
}

pub struct NopTracer;
impl Tracer for NopTracer {
    fn notify(&mut self, _event: &TraceEvent, _writer: Writer<'_>) {}
}

/// A writer that allows you to push custom events to the trace but encapsulates the trace so that
/// non-external events cannot be accidentally added.
pub struct Writer<'a>(pub(crate) &'a mut MoveTrace);

impl<'a> Writer<'a> {
    /// Emit an external event into the trace.
    pub fn push<T: Serialize>(&mut self, e: T) {
        self.0.events.push(TraceEvent::External(Box::new(
            serde_json::to_value(e).unwrap(),
        )));
    }

    /// Get the current version of the trace.
    pub fn trace_version(&mut self) -> TraceVersion {
        self.0.version
    }
}
