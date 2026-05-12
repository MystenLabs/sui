// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::format::TraceEvent;
use crate::interface::{Tracer, Writer};
use move_core_types::language_storage::ModuleId;

pub struct NopTracer;
impl Tracer for NopTracer {
    fn notify(&mut self, _event: &TraceEvent, _writer: Writer<'_>) -> bool {
        // keep all events
        true
    }

    fn wants_effects(&self) -> bool {
        true
    }

    fn wants_frame_snapshot(&self, _module: &ModuleId) -> bool {
        true
    }
}
