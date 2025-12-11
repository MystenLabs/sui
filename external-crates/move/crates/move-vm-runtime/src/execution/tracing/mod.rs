// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

pub mod tracer;

/// Invoke a tracing operation when the `tracing` feature is enabled.
///
/// This macro provides zero-cost tracing when the `tracing` feature is disabled:
///
/// Using a macro instead of a generic function ensures that:
/// 1. No closure types are monomorphized when tracing is disabled
/// 2. No VMTracer method references appear in the binary
/// 3. The tracer parameter type can be `()` when disabled (zero-sized)
///
/// # Usage
/// ```ignore
/// trace!(run_context.tracer, |tracer| {
///     tracer.start_instruction(vtables, state, &gas_meter.remaining_gas().into())
/// });
/// ```
#[cfg(feature = "tracing")]
macro_rules! trace {
    ($tracer:expr, |$param:ident| $body:expr) => {
        if let Some($param) = $tracer.as_mut() {
            $body
        }
    };
}

/// No-op version.
#[cfg(not(feature = "tracing"))]
macro_rules! trace {
    ($tracer:expr, |$param:ident| $body:expr) => {
        // Intentionally empty - tracing disabled at compile time
        // The $body is captured but never expanded, so no code is generated
        let _ = &$tracer; // Suppress unused warning without evaluating
    };
}

pub(crate) use trace;
