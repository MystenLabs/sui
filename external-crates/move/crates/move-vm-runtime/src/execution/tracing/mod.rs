// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use self::tracer::VMTracer;

pub mod tracer;

#[cfg(feature = "tracing")]
pub(crate) const TRACING_ENABLED: bool = true;

#[cfg(not(feature = "tracing"))]
pub(crate) const TRACING_ENABLED: bool = false;

#[inline]
pub(crate) fn trace<'a, F: Fn(&mut VMTracer<'a>) -> ()>(tracer: &mut Option<VMTracer<'a>>, op: F) {
    if TRACING_ENABLED {
        if let Some(tracer) = tracer {
            op(tracer)
        }
    }
}
