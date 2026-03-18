// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::format::TraceEvent;
use crate::interface::{Tracer, Writer};

/// A tracer that only keeps `Instruction`, `OpenFrame`, and `CloseFrame` events.
pub struct InstructionOnlyTracer;
impl Tracer for InstructionOnlyTracer {
    fn notify(&mut self, event: &TraceEvent, _writer: Writer<'_>) -> bool {
        matches!(
            event,
            TraceEvent::Instruction { .. }
                | TraceEvent::OpenFrame { .. }
                | TraceEvent::CloseFrame { .. }
        )
    }
}
