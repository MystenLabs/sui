// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

// Debug print code for development. Remove when moving into production mode.
pub const DEBUG_PRINT: bool = true;

pub struct DebugFlags {
    pub function_list_sizes: bool,
    pub function_resolution: bool,
    pub eval_step: bool,
    pub optimizer: bool,
}

pub const DEBUG_FLAGS: DebugFlags = DebugFlags {
    function_list_sizes: false,
    function_resolution: false,
    eval_step: false,
    optimizer: true,
};

#[cfg(debug_assertions)]
#[macro_export]
macro_rules! dbg_println {
    ($( $args:expr ),*$(,)?) => {
        if $crate::dev_utils::dbg_print::DEBUG_PRINT {
            println!( $( $args ),* );
        }
    };
    (flag: $field:ident, $( $args:expr ),*$(,)?) => {
        if $crate::dev_utils::dbg_print::DEBUG_PRINT && $crate::dev_utils::dbg_print::DEBUG_FLAGS.$field {
            println!( $( $args ),* );
        }
    };
}

#[cfg(not(debug_assertions))]
#[macro_export]
macro_rules! dbg_println {
    ($( $args:expr ),*$(,)?) => {};
    (flag: $field:ident, $( $args:expr ),*$(,)?) => {};
}
