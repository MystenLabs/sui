// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    format::TraceEvent,
    interface::{EventFilter, Tracer, Writer},
};
use move_binary_format::file_format_common::Opcodes;

pub struct NopTracer;
impl Tracer for NopTracer {
    fn notify(&mut self, _event: &TraceEvent, _writer: Writer<'_>) -> bool {
        // keep all events
        true
    }

    fn instruction_filter(&self, _instruction: &Opcodes, _pc: u16) -> Option<EventFilter> {
        Some(|_event_index| true)
    }
}
