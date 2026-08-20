// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    format::TraceEvent,
    interface::{EventFilter, Tracer, Writer},
};
use move_binary_format::file_format_common::Opcodes;

pub struct EffectsOnlyTracer {
    filter: EventFilter,
}

impl EffectsOnlyTracer {
    pub fn pre() -> Self {
        Self {
            filter: |event_index| event_index < 0,
        }
    }

    pub fn post() -> Self {
        Self {
            filter: |event_index| event_index > 0,
        }
    }
}

impl Tracer for EffectsOnlyTracer {
    fn notify(&mut self, _event: &TraceEvent, _writer: Writer<'_>) -> bool {
        true
    }

    fn instruction_filter(&self, _instruction: &Opcodes, _pc: u16) -> Option<EventFilter> {
        Some(self.filter)
    }
}
