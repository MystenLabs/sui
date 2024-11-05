pub(crate) mod tracer;

#[cfg(feature = "tracing")]
pub(crate) const TRACING_ENABLED: bool = true;

#[cfg(not(feature = "tracing"))]
pub(crate) const TRACING_ENABLED: bool = false;

#[macro_export]
macro_rules! open_initial_frame {
    ($tracer: expr, $args: expr, $ty_args: expr, $function: expr, $loader: expr, $gas_meter: expr, $link_context: expr) => {
        if $crate::tracing2::TRACING_ENABLED {
            $tracer.as_mut().map(|tracer| {
                tracer.open_initial_frame(
                    $args,
                    $ty_args,
                    $function,
                    $loader,
                    $gas_meter.remaining_gas().into(),
                    $link_context,
                )
            });
            move_vm_profiler::profile_open_frame!($gas_meter, $function.pretty_string());
        }
    };
}

#[macro_export]
macro_rules! close_initial_frame {
    ($tracer: expr, $function: expr, $return_values: expr, $gas_meter: expr) => {
        if $crate::tracing2::TRACING_ENABLED {
            $tracer.as_mut().map(|tracer| {
                tracer.close_initial_frame($return_values, $gas_meter.remaining_gas().into())
            });
            move_vm_profiler::profile_close_frame!($gas_meter, $function.pretty_string());
        }
    };
}

#[macro_export]
macro_rules! close_frame {
    ($tracer: expr, $frame: expr, $function: expr, $interp: expr, $loader: expr, $gas_meter: expr, $link_context: expr, $call_err: expr) => {
        if $crate::tracing2::TRACING_ENABLED {
            $tracer.as_mut().map(|tracer| {
                tracer.close_frame(
                    $frame,
                    $function,
                    $interp,
                    $loader,
                    $gas_meter.remaining_gas().into(),
                    $link_context,
                    $call_err,
                )
            });
            move_vm_profiler::profile_close_frame!($gas_meter, $function.pretty_string());
        }
    };
}

#[macro_export]
macro_rules! open_frame {
    ($tracer: expr, $ty_args: expr, $function: expr, $calling_frame: expr, $interp: expr, $loader: expr, $gas_meter: expr, $link_context: expr) => {
        if $crate::tracing2::TRACING_ENABLED {
            $tracer.as_mut().map(|tracer| {
                tracer.open_frame(
                    $ty_args,
                    $function,
                    $calling_frame,
                    $interp,
                    $loader,
                    $gas_meter.remaining_gas().into(),
                    $link_context,
                )
            });
            move_vm_profiler::profile_open_frame!($gas_meter, $function.pretty_string());
        }
    };
}

#[macro_export]
macro_rules! open_instruction {
    ($tracer: expr, $instruction: expr, $frame: expr, $interp: expr, $loader: expr, $gas_meter: expr) => {
        if $crate::tracing2::TRACING_ENABLED {
            $tracer.as_mut().map(|tracer| {
                tracer.open_instruction($frame, $interp, $loader, $gas_meter.remaining_gas().into())
            });
            move_vm_profiler::profile_open_instr!($gas_meter, format!("{:?}", $instruction));
        }
    };
}

#[macro_export]
macro_rules! close_instruction {
    ($tracer: expr, $instruction: expr, $frame: expr, $interp: expr, $loader: expr, $gas_meter: expr, $result: expr) => {
        if $crate::tracing2::TRACING_ENABLED {
            $tracer.as_mut().map(|tracer| {
                tracer.close_instruction(
                    $frame,
                    $interp,
                    $loader,
                    $gas_meter.remaining_gas().into(),
                    $result,
                )
            });
            move_vm_profiler::profile_close_instr!($gas_meter, format!("{:?}", $instruction));
        }
    };
}
