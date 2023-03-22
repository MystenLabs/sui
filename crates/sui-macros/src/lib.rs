// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::future::BoxFuture;
use std::collections::HashMap;
use std::future::Future;
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

type FpCallback = dyn Fn() -> Option<BoxFuture<'static, ()>> + Send + Sync + 'static;
type FpMap = HashMap<&'static str, Arc<FpCallback>>;

#[cfg(msim)]
fn with_fp_map<T>(func: impl FnOnce(&mut FpMap) -> T) -> T {
    thread_local! {
        static MAP: std::cell::RefCell<FpMap> = Default::default();
    }

    MAP.with(|val| func(&mut val.borrow_mut()))
}

#[cfg(not(msim))]
fn with_fp_map<T>(func: impl FnOnce(&mut FpMap) -> T) -> T {
    use once_cell::sync::Lazy;
    use std::sync::Mutex;

    static MAP: Lazy<Mutex<FpMap>> = Lazy::new(Default::default);
    let mut map = MAP.lock().unwrap();
    func(&mut map)
}

fn get_callback(identifier: &'static str) -> Option<Arc<FpCallback>> {
    with_fp_map(|map| map.get(identifier).cloned())
}

pub fn handle_fail_point(identifier: &'static str) {
    if let Some(callback) = get_callback(identifier) {
        tracing::error!("hit failpoint {}", identifier);
        assert!(
            callback().is_none(),
            "sync failpoint must not return future"
        );
    }
}

pub async fn handle_fail_point_async(identifier: &'static str) {
    if let Some(callback) = get_callback(identifier) {
        tracing::error!("hit async failpoint {}", identifier);
        let fut = callback().expect("async callback must return future");
        fut.await;
    }
}

fn register_fail_point_impl(
    identifier: &'static str,
    callback: Arc<dyn Fn() -> Option<BoxFuture<'static, ()>> + Sync + Send + 'static>,
) {
    with_fp_map(move |map| {
        assert!(
            map.insert(identifier, callback).is_none(),
            "duplicate fail point registration"
        );
    })
}

pub fn register_fail_point(identifier: &'static str, callback: impl Fn() + Sync + Send + 'static) {
    register_fail_point_impl(
        identifier,
        Arc::new(move || {
            callback();
            None
        }),
    );
}

pub fn register_fail_point_async<F>(
    identifier: &'static str,
    callback: impl Fn() -> F + Sync + Send + 'static,
) where
    F: Future<Output = ()> + Sync + Send + 'static,
{
    register_fail_point_impl(identifier, Arc::new(move || Some(Box::pin(callback()))));
}

pub fn register_fail_points(
    identifiers: &[&'static str],
    callback: impl Fn() + Sync + Send + 'static,
) {
    let cb = Arc::new(move || {
        callback();
        None
    });
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

#[cfg(any(msim, fail_points))]
#[macro_export]
macro_rules! fail_point_async {
    ($tag: expr) => {
        $crate::handle_fail_point_async($tag).await
    };
}

#[cfg(not(any(msim, fail_points)))]
#[macro_export]
macro_rules! fail_point {
    ($tag: expr) => {};
}

#[cfg(not(any(msim, fail_points)))]
#[macro_export]
macro_rules! fail_point_async {
    ($tag: expr) => {};
}
