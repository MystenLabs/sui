#[macro_export]
macro_rules! trace_open_main_frame {
    (
        $tracer:expr,
        $args:expr,
        $ty_args:expr,
        $function:expr,
        $loader:expr,
        $gas_remaining:expr,
        $link_context:expr
        $(,)?
    ) => {
        // Only include this code in debug releases
        #[cfg(any(debug_assertions, feature = "debugging"))]
        $tracer.as_mut().map(|tracer| {
            tracer.open_main_frame(
                $args,
                $ty_args,
                $function,
                $loader,
                $gas_remaining,
                $link_context,
            )
        })
    };
}

#[macro_export]
macro_rules! trace_close_main_frame {
    (
        $tracer:expr,
        $ty_args:expr,
        $return_values:expr,
        $function:expr,
        $loader:expr,
        $gas_remaining:expr,
        $link_context:expr
        $(,)?
    ) => {
        // Only include this code in debug releases
        #[cfg(any(debug_assertions, feature = "debugging"))]
        $tracer.as_mut().map(|tracer| {
            tracer.close_main_frame(
                $ty_args,
                $return_values,
                $function,
                $loader,
                $gas_remaining,
                $link_context,
            )
        })
    };
}

#[macro_export]
macro_rules! trace_open_frame {
    (
        $tracer:expr,
        $ty_args:expr,
        $function:expr,
        $frame:expr,
        $interp:expr,
        $loader:expr,
        $gas_remaining:expr,
        $link_context:expr
        $(,)?
    ) => {
        // Only include this code in debug releases
        #[cfg(any(debug_assertions, feature = "debugging"))]
        $tracer.as_mut().map(|tracer| {
            tracer.open_frame(
                $ty_args,
                $function,
                $frame,
                $interp,
                $loader,
                $gas_remaining,
                $link_context,
            )
        })
    };
}

#[macro_export]
macro_rules! trace_close_frame {
    (
        $tracer:expr,
        $frame:expr,
        $func:expr,
        $interp:expr,
        $loader:expr,
        $gas_remaining:expr,
        $link_context:expr
        $(,)?
    ) => {
        // Only include this code in debug releases
        #[cfg(any(debug_assertions, feature = "debugging"))]
        $tracer.as_mut().map(|tracer| {
            tracer.close_frame(
                $frame,
                $func,
                $interp,
                $loader,
                $gas_remaining,
                $link_context,
            )
        })
    };
}

#[macro_export]
macro_rules! trace_open_instruction {
    (
        $tracer:expr,
        $frame:expr,
        $interp:expr,
        $loader:expr,
        $gas_remaining:expr
        $(,)?
    ) => {
        // Only include this code in debug releases
        #[cfg(any(debug_assertions, feature = "debugging"))]
        $tracer
            .as_mut()
            .map(|tracer| tracer.open_instruction($frame, $interp, $loader, $gas_remaining))
    };
}

#[macro_export]
macro_rules! trace_close_instruction {
    (
        $tracer:expr,
        $frame:expr,
        $interp:expr,
        $loader:expr,
        $gas_remaining:expr
        $(,)?
    ) => {
        // Only include this code in debug releases
        #[cfg(any(debug_assertions, feature = "debugging"))]
        $tracer
            .as_mut()
            .map(|tracer| tracer.close_instruction($frame, $interp, $loader, $gas_remaining))
    };
}
