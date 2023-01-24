// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;
use std::sync::Arc;

pub use sui_proc_macros::*;

/// Evaluates an expression in a new thread which will not be subject to interception of
/// getrandom(), clock_gettime(), etc.
#[cfg(msim)]
#[macro_export]
macro_rules! nondeterministic {
    ($expr: expr) => {
        std::thread::scope(move |s| s.spawn(move || $expr).join().unwrap())
    };
}

/// Simply evaluates expr.
#[cfg(not(msim))]
#[macro_export]
macro_rules! nondeterministic {
    ($expr: expr) => {
        $expr
    };
}

type FpMap = HashMap<&'static str, Arc<dyn Fn() + Sync + Send + 'static>>;

#[cfg(msim)]
fn with_fp_map(func: impl FnOnce(&mut FpMap)) {
    thread_local! {
        static MAP: std::cell::RefCell<FpMap> = Default::default();
    }

    MAP.with(|val| {
        func(&mut val.borrow_mut());
    })
}

#[cfg(not(msim))]
fn with_fp_map(func: impl FnOnce(&mut FpMap)) {
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    static MAP: Lazy<Mutex<FpMap>> = Lazy::new(Default::default);
    let mut map = MAP.lock().unwrap();
    func(&mut map);
}

pub fn handle_fail_point(identifier: &'static str) {
    with_fp_map(|map| {
        if let Some(callback) = map.get(identifier) {
            callback();
        }
    })
}

fn register_fail_point_impl(
    identifier: &'static str,
    callback: Arc<dyn Fn() + Sync + Send + 'static>,
) {
    with_fp_map(move |map| {
        assert!(
            map.insert(identifier, callback).is_none(),
            "duplicate fail point registration"
        );
    })
}

pub fn register_fail_point(identifier: &'static str, callback: impl Fn() + Sync + Send + 'static) {
    register_fail_point_impl(identifier, Arc::new(callback));
}

pub fn register_fail_points(
    identifiers: &[&'static str],
    callback: impl Fn() + Sync + Send + 'static,
) {
    let cb = Arc::new(callback);
    for id in identifiers {
        register_fail_point_impl(id, cb.clone());
    }
}

#[cfg(any(msim, fail_points))]
#[macro_export]
macro_rules! fail_point {
    ($tag: expr) => {
        $crate::handle_fail_point($tag)
    };
}

#[cfg(not(any(msim, fail_points)))]
#[macro_export]
macro_rules! fail_point {
    ($tag: expr) => {};
}
