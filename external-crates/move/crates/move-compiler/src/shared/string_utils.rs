// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//**************************************************************************************************
// String Validation Helpers
//**************************************************************************************************

pub fn is_pascal_case(s: &str) -> bool {
    let mut iter = s.chars();
    let Some(start) = iter.next() else {
        return true;
    };
    start.is_uppercase() && iter.all(|c| c.is_alphanumeric())
}

pub fn is_upper_snake_case(s: &str) -> bool {
    s.chars()
        .all(|c| c.is_uppercase() || c.is_numeric() || c == '_')
}

//**************************************************************************************************
// String Construction Helpers
//**************************************************************************************************

macro_rules! format_oxford_list {
    ($sep:expr, $format_str:expr, $e:expr) => {{
        let entries = $e;
        match entries.len() {
            0 => String::new(),
            1 => format!($format_str, entries[0]),
            2 => format!(
                "{} {} {}",
                format!($format_str, entries[0]),
                $sep,
                format!($format_str, entries[1])
            ),
            _ => {
                let entries = entries
                    .iter()
                    .map(|entry| format!($format_str, entry))
                    .collect::<Vec<_>>();
                if let Some((last, init)) = entries.split_last() {
                    let mut result = init.join(", ");
                    result.push_str(&format!(", {} {}", $sep, last));
                    result
                } else {
                    String::new()
                }
            }
        }
    }};
}

pub(crate) use format_oxford_list;

//**************************************************************************************************
// Debug Printing
//**************************************************************************************************

/// Debug formatter based on provided `fmt` option:
///
/// - None: calls `val.print()`
/// - `verbose`: calls `val.print_verbose()`
/// - `fmt`: calls `println!("{}", val)`
/// - `dbg`: calls `println!("{:?}", val)`
/// - `sdbg`: calls `println!("{:#?}", val)`
macro_rules! debug_print_format {
    ($val:expr) => {{
        $val.print();
    }};
    ($val:expr ; verbose) => {{
        $val.print_verbose();
    }};
    ($val:expr ; fmt) => {{
        println!("{}", $val);
    }};
    ($val:expr ; dbg) => {{
        println!("{:?}", $val);
    }};
    ($val:expr ; sdbg) => {{
        println!("{:#?}", $val);
    }};
}

pub(crate) use debug_print_format;

/// Print formatter for debugging. Allows a few different forms:
///
/// `(msg `s`)`                        as println!(s);
/// `(name => val [; fmt])`            as "name: " + debug_fprint_ormat!(vall fmt)
/// `(opt name => val [; fmt])`        as "name: " + "Some " debug_print_format!(val; fmt) or "None"
/// `(lines name => val [; fmt]) ` as "name: " + for n in val { debug_print_format!(n; fmt) }
///
/// See `debug_print_format` for different `fmt` options.
macro_rules! debug_print_internal {
    () => {};
    (($name:expr => $val:expr $(; $fmt:ident)?)) => {
        {
        print!("{}: ", $name);
        crate::shared::string_utils::debug_print_format!($val $(; $fmt)*);
        }
    };
    ((msg $val:expr)) => {
        {
            println!("{}", $val);
        }
    };
    ((opt $name:expr => $val:expr $(; $fmt:ident)?)) => {
        {
        print!("{}: ", $name);
        match $val {
            Some(value) => { print!("Some "); crate::shared::string_utils::debug_print_format!(value $(; $fmt)*); }
            None => { print!("None"); }
        }
        }
    };
    ((lines $name:expr => $val:expr $(; $fmt:ident)?)) => { {
        println!("\n{}: ", $name);
        for n in $val {
            crate::shared::string_utils::debug_print_format!(n $(; $fmt)*);
        }
    }
    };
    ($fst:tt, $($rest:tt),+) => { {
        crate::shared::string_utils::debug_print_internal!($fst);
        crate::shared::string_utils::debug_print_internal!($($rest),+);
    }
    };
}

pub(crate) use debug_print_internal;

/// Macro for a small DSL for compactling printing debug information based on the provided flag.
///
///  ```
///  debug_print!(
///      context.debug_flags.match_compilation,
///      ("subject" => subject),
///      (opt "flag" => flag; dbg)
///      (lines "arms" => &arms.value; verbose)
///  );
///  ```
///
/// See `debug_print_internal` for the available syntax.
///
/// Feature gates the print and check against the `debug_assertions` feature.
macro_rules! debug_print {
    ($flag:expr, $($arg:tt),+) => {
        #[cfg(debug_assertions)]
        if $flag {
            println!("\n------------------");
            crate::shared::string_utils::debug_print_internal!($($arg),+)
        }
    }
}

pub(crate) use debug_print;
