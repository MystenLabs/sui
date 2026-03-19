// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::format::TraceEvent;
use crate::interface::{Tracer, Writer};

/// A tracer that only keeps `OpenFrame`, and `CloseFrame` events.
pub struct FunctionOnlyTracer;
impl Tracer for FunctionOnlyTracer {
    fn notify(&mut self, event: &TraceEvent, _writer: Writer<'_>) -> bool {
        matches!(
            event,
            TraceEvent::OpenFrame { .. } | TraceEvent::CloseFrame { .. }
        )
    }

    fn wants_effects(&self) -> bool {
        false
    }
}
