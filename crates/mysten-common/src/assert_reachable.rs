// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use once_cell::sync::Lazy;
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct ReachableAssertion {
    pub assertion_type: &'static str,
    pub loc: &'static str,
    pub msg: &'static str,
}

static REACHABLE_LOG_DIR: Lazy<Option<PathBuf>> = Lazy::new(|| {
    std::env::var("MSIM_LOG_REACHABLE_ASSERTIONS")
        .ok()
        .map(PathBuf::from)
        .filter(|p| p.is_dir())
});

static REACHABLE_LOG_FILE: Lazy<Option<Mutex<std::fs::File>>> = Lazy::new(|| {
    let dir = REACHABLE_LOG_DIR.as_ref()?;
    let seed = std::env::var("MSIM_TEST_SEED").unwrap_or_else(|_| "unknown".to_string());
    let filename = format!("{}.reached", seed);
    let path = dir.join(filename);
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
        .map(Mutex::new)
});

static SOMETIMES_LOG_FILE: Lazy<Option<Mutex<std::fs::File>>> = Lazy::new(|| {
    let dir = REACHABLE_LOG_DIR.as_ref()?;
    let seed = std::env::var("MSIM_TEST_SEED").unwrap_or_else(|_| "unknown".to_string());
    let filename = format!("{}.sometimes", seed);
    let path = dir.join(filename);
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .ok()
        .map(Mutex::new)
});

pub fn log_reached_assertion(loc: &'static str) {
    if let Some(file) = REACHABLE_LOG_FILE.as_ref()
        && let Ok(mut f) = file.lock()
    {
        let _ = writeln!(f, "{}", loc);
    }
}

pub fn log_sometimes_assertion(loc: &'static str) {
    if let Some(file) = SOMETIMES_LOG_FILE.as_ref()
        && let Ok(mut f) = file.lock()
    {
        let _ = writeln!(f, "{}", loc);
    }
}

#[macro_export]
macro_rules! assert_reachable_simtest_impl {
    ($assertion_type:literal, $condition:expr, $message:literal, $log_fn:path) => {{
        use std::sync::atomic::{AtomicBool, Ordering};
        use $crate::assert_reachable::ReachableAssertion;

        const LOC: &str = concat!(file!(), ":", line!(), ":", column!());

        #[used]
        #[cfg_attr(
            any(target_os = "linux", target_os = "android"),
            unsafe(link_section = ".reach_points")
        )]
        #[cfg_attr(target_os = "macos", unsafe(link_section = "__DATA,__reach_points"))]
        #[cfg_attr(target_os = "windows", unsafe(link_section = ".reach_points"))]
        static RP: ReachableAssertion = ReachableAssertion {
            assertion_type: $assertion_type,
            loc: LOC,
            msg: $message,
        };

        if $condition {
            static LOGGED: AtomicBool = AtomicBool::new(false);
            if !LOGGED.swap(true, Ordering::Relaxed) {
                $log_fn(LOC);
            }
        }
    }};
}

#[macro_export]
macro_rules! assert_reachable_simtest {
    ($message:literal) => {
        $crate::assert_reachable_simtest_impl!(
            "reachable",
            true,
            $message,
            $crate::assert_reachable::log_reached_assertion
        )
    };
}

#[macro_export]
macro_rules! assert_sometimes_simtest {
    ($expr:expr, $message:literal) => {
        $crate::assert_reachable_simtest_impl!(
            "sometimes",
            $expr,
            $message,
            $crate::assert_reachable::log_sometimes_assertion
        )
    };
}
