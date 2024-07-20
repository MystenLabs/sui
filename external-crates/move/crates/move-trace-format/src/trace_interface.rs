// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::trace_format::{MoveTrace, TraceEvent, TraceVersion};
use serde::Serialize;

pub trait Tracer {
    fn notify<'a>(&mut self, event: &TraceEvent, writer: Writer<'a>);
}

pub struct NopTracer;
impl Tracer for NopTracer {
    fn notify<'a>(&mut self, _event: &TraceEvent, _writer: Writer<'a>) {}
}

pub struct Writer<'a>(pub(crate) &'a mut MoveTrace);

impl<'a> Writer<'a> {
    pub fn push<T: Serialize>(&mut self, e: T) {
        self.0
            .events
            .push(TraceEvent::External(serde_json::to_value(e).unwrap()));
    }

    pub fn trace_version(&mut self) -> TraceVersion {
        self.0.version
    }
}
