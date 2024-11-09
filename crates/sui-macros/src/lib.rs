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

type FpCallback = dyn Fn() -> Box<dyn std::any::Any + Send + 'static> + Send + Sync;
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

fn get_sync_fp_result(result: Box<dyn std::any::Any + Send + 'static>) {
    if result.downcast::<()>().is_err() {
        panic!("sync failpoint must return ()");
    }
}

fn get_async_fp_result(result: Box<dyn std::any::Any + Send + 'static>) -> BoxFuture<'static, ()> {
    match result.downcast::<BoxFuture<'static, ()>>() {
        Ok(fut) => *fut,
        Err(err) => panic!(
            "async failpoint must return BoxFuture<'static, ()> {:?}",
            err
        ),
    }
}

fn get_fp_if_result(result: Box<dyn std::any::Any + Send + 'static>) -> bool {
    match result.downcast::<bool>() {
        Ok(b) => *b,
        Err(_) => panic!("failpoint-if must return bool"),
    }
}

fn get_fp_some_result<T: Send + 'static>(
    result: Box<dyn std::any::Any + Send + 'static>,
) -> Option<T> {
    match result.downcast::<Option<T>>() {
        Ok(opt) => *opt,
        Err(_) => panic!("failpoint-arg must return Option<T>"),
    }
}

pub fn handle_fail_point(identifier: &'static str) {
    if let Some(callback) = get_callback(identifier) {
        get_sync_fp_result(callback());
        tracing::trace!("hit failpoint {}", identifier);
    }
}

pub async fn handle_fail_point_async(identifier: &'static str) {
    if let Some(callback) = get_callback(identifier) {
        tracing::trace!("hit async failpoint {}", identifier);
        let fut = get_async_fp_result(callback());
        fut.await;
    }
}

pub fn handle_fail_point_if(identifier: &'static str) -> bool {
    if let Some(callback) = get_callback(identifier) {
        tracing::trace!("hit failpoint_if {}", identifier);
        get_fp_if_result(callback())
    } else {
        false
    }
}

pub fn handle_fail_point_arg<T: Send + 'static>(identifier: &'static str) -> Option<T> {
    if let Some(callback) = get_callback(identifier) {
        tracing::trace!("hit failpoint_arg {}", identifier);
        get_fp_some_result(callback())
    } else {
        None
    }
}

fn register_fail_point_impl(identifier: &'static str, callback: Arc<FpCallback>) {
    with_fp_map(move |map| {
        assert!(
            map.insert(identifier, callback).is_none(),
            "duplicate fail point registration"
        );
    })
}

fn clear_fail_point_impl(identifier: &'static str) {
    with_fp_map(move |map| {
        assert!(
            map.remove(identifier).is_some(),
            "fail point {:?} does not exist",
            identifier
        );
    })
}

pub fn register_fail_point(identifier: &'static str, callback: impl Fn() + Sync + Send + 'static) {
    register_fail_point_impl(
        identifier,
        Arc::new(move || {
            callback();
            Box::new(())
        }),
    );
}

/// Register an asynchronous fail point. Because it is async it can yield execution of the calling
/// task, e.g. by sleeping.
pub fn register_fail_point_async<F>(
    identifier: &'static str,
    callback: impl Fn() -> F + Sync + Send + 'static,
) where
    F: Future<Output = ()> + Send + 'static,
{
    register_fail_point_impl(
        identifier,
        Arc::new(move || {
            let result: BoxFuture<'static, ()> = Box::pin(callback());
            Box::new(result)
        }),
    );
}

/// Register code to run locally if the fail point is hit. Example:
///
/// In the test:
///
/// ```ignore
///     register_fail_point_if("foo", || {
///         sui_simulator::current_simnode_id() == 2
///     });
/// ```
///
/// In the code:
///
/// ```ignore
///     let mut was_hit = false;
///     fail_point_if("foo", || {
///        was_hit = true;
///     });
/// ```
pub fn register_fail_point_if(
    identifier: &'static str,
    callback: impl Fn() -> bool + Sync + Send + 'static,
) {
    register_fail_point_impl(identifier, Arc::new(move || Box::new(callback())));
}

/// Register code to run locally if the fail point is hit, with a value provided
/// by the test. If the registered callback returns a Some(v), then the `v` is
/// passed to the callback in the test.
///
/// In the test:
///
/// ```ignore
///     register_fail_point_arg("foo", || {
///         Some(42)
///     });
/// ```
///
/// In the code:
///
/// ```ignore
///     let mut value = 0;
///     fail_point_arg!("foo", |arg| {
///        value = arg;
///     });
/// ```
pub fn register_fail_point_arg<T: Send + 'static>(
    identifier: &'static str,
    callback: impl Fn() -> Option<T> + Sync + Send + 'static,
) {
    register_fail_point_impl(identifier, Arc::new(move || Box::new(callback())));
}

pub fn register_fail_points(
    identifiers: &[&'static str],
    callback: impl Fn() + Sync + Send + 'static,
) {
    let cb: Arc<FpCallback> = Arc::new(move || {
        callback();
        Box::new(())
    });
    for id in identifiers {
        register_fail_point_impl(id, cb.clone());
    }
}

pub fn clear_fail_point(identifier: &'static str) {
    clear_fail_point_impl(identifier);
}

/// Trigger a fail point. Tests can trigger various behavior when the fail point is hit.
#[cfg(any(msim, fail_points))]
#[macro_export]
macro_rules! fail_point {
    ($tag: expr) => {
        $crate::handle_fail_point($tag)
    };
}

/// Trigger an async fail point. Tests can trigger various async behavior when the fail point is
/// hit.
#[cfg(any(msim, fail_points))]
#[macro_export]
macro_rules! fail_point_async {
    ($tag: expr) => {
        $crate::handle_fail_point_async($tag).await
    };
}

/// Trigger a failpoint that runs a callback at the callsite if it is enabled.
/// (whether it is enabled is controlled by whether the registration callback returns true/false).
#[cfg(any(msim, fail_points))]
#[macro_export]
macro_rules! fail_point_if {
    ($tag: expr, $callback: expr) => {
        if $crate::handle_fail_point_if($tag) {
            ($callback)();
        }
    };
}

/// Trigger a failpoint that runs a callback at the callsite if it is enabled.
/// If the registration callback returns Some(v), then the `v` is passed to the callback in the test.
/// Otherwise the failpoint is skipped
#[cfg(any(msim, fail_points))]
#[macro_export]
macro_rules! fail_point_arg {
    ($tag: expr, $callback: expr) => {
        if let Some(arg) = $crate::handle_fail_point_arg($tag) {
            ($callback)(arg);
        }
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

#[cfg(not(any(msim, fail_points)))]
#[macro_export]
macro_rules! fail_point_if {
    ($tag: expr, $callback: expr) => {};
}

#[cfg(not(any(msim, fail_points)))]
#[macro_export]
macro_rules! fail_point_arg {
    ($tag: expr, $callback: expr) => {};
}

/// Use to write INFO level logs only when REPLAY_LOG
/// environment variable is set. Useful for log lines that
/// are only relevant to test infra which still may need to
/// run a release build. Also note that since logs of a chain
/// replay are exceedingly verbose, this will allow one to bubble
/// up "debug level" info while running with RUST_LOG=info.
#[macro_export]
macro_rules! replay_log {
    ($($arg:tt)+) => {
        if std::env::var("REPLAY_LOG").is_ok() {
            tracing::info!($($arg)+);
        }
    };
}

// These tests need to be run in release mode, since debug mode does overflow checks by default!
#[cfg(test)]
mod test {
    use super::*;

    // Uncomment to test error messages
    // #[with_checked_arithmetic]
    // struct TestStruct;

    macro_rules! pass_through {
        ($($tt:tt)*) => {
            $($tt)*
        }
    }

    #[with_checked_arithmetic]
    #[test]
    fn test_skip_checked_arithmetic() {
        // comment out this attr to test the error message
        #[skip_checked_arithmetic]
        pass_through! {
            fn unchecked_add(a: i32, b: i32) -> i32 {
                a + b
            }
        }

        // this will not panic even if we pass in (i32::MAX, 1), because we skipped processing
        // the item macro, so we also need to make sure it doesn't panic in debug mode.
        unchecked_add(1, 2);
    }

    checked_arithmetic! {

    struct Test {
        a: i32,
        b: i32,
    }

    fn unchecked_add(a: i32, b: i32) -> i32 {
        a + b
    }

    #[test]
    fn test_checked_arithmetic_macro() {
        unchecked_add(1, 2);
    }

    #[test]
    #[should_panic]
    fn test_checked_arithmetic_macro_panic() {
        unchecked_add(i32::MAX, 1);
    }

    fn unchecked_add_hidden(a: i32, b: i32) -> i32 {
        let inner = |a: i32, b: i32| a + b;
        inner(a, b)
    }

    #[test]
    #[should_panic]
    fn test_checked_arithmetic_macro_panic_hidden() {
        unchecked_add_hidden(i32::MAX, 1);
    }

    fn unchecked_add_hidden_2(a: i32, b: i32) -> i32 {
        fn inner(a: i32, b: i32) -> i32 {
            a + b
        }
        inner(a, b)
    }

    #[test]
    #[should_panic]
    fn test_checked_arithmetic_macro_panic_hidden_2() {
        unchecked_add_hidden_2(i32::MAX, 1);
    }

    impl Test {
        fn add(&self) -> i32 {
            self.a + self.b
        }
    }

    #[test]
    #[should_panic]
    fn test_checked_arithmetic_impl() {
        let t = Test { a: 1, b: i32::MAX };
        t.add();
    }

    #[test]
    #[should_panic]
    fn test_macro_overflow() {
        #[allow(arithmetic_overflow)]
        fn f() {
            println!("{}", i32::MAX + 1);
        }

        f()
    }

    // Make sure that we still do addition correctly!
    #[test]
    fn test_non_overflow() {
        fn f() {
            assert_eq!(1i32 + 2i32, 3i32);
            assert_eq!(3i32 - 1i32, 2i32);
            assert_eq!(4i32 * 3i32, 12i32);
            assert_eq!(12i32 / 3i32, 4i32);
            assert_eq!(12i32 % 5i32, 2i32);

            let mut a = 1i32;
            a += 2i32;
            assert_eq!(a, 3i32);

            let mut a = 3i32;
            a -= 1i32;
            assert_eq!(a, 2i32);

            let mut a = 4i32;
            a *= 3i32;
            assert_eq!(a, 12i32);

            let mut a = 12i32;
            a /= 3i32;
            assert_eq!(a, 4i32);

            let mut a = 12i32;
            a %= 5i32;
            assert_eq!(a, 2i32);
        }

        f();
    }


    #[test]
    fn test_exprs_evaluated_once_right() {
        let mut called = false;
        let mut f = || {
            if called {
                panic!("called twice");
            }
            called = true;
            1i32
        };

        assert_eq!(2i32 + f(), 3);
    }

    #[test]
    fn test_exprs_evaluated_once_left() {
        let mut called = false;
        let mut f = || {
            if called {
                panic!("called twice");
            }
            called = true;
            1i32
        };

        assert_eq!(f() + 2i32, 3);
    }

    #[test]
    fn test_assign_op_evals_once() {
        struct Foo {
            a: i32,
            called: bool,
        }

        impl Foo {
            fn get_a_mut(&mut self) -> &mut i32 {
                if self.called {
                    panic!("called twice");
                }
                let ret = &mut self.a;
                self.called = true;
                ret
            }
        }

        let mut foo = Foo { a: 1, called: false };

        *foo.get_a_mut() += 2;
        assert_eq!(foo.a, 3);
    }

    #[test]
    fn test_more_macro_syntax() {
        struct Foo {
            a: i32,
            b: i32,
        }

        impl Foo {
            const BAR: i32 = 1;

            fn new(a: i32, b: i32) -> Foo {
                Foo { a, b }
            }
        }

        fn new_foo(a: i32) -> Foo {
            Foo { a, b: 0 }
        }

        // verify that we translate the contents of macros correctly
        assert_eq!(Foo::BAR + 1, 2);
        assert_eq!(Foo::new(1, 2).b, 2);
        assert_eq!(new_foo(1).a, 1);

        let v = [Foo::new(1, 2), Foo::new(3, 2)];

        assert_eq!(v[0].a, 1);
        assert_eq!(v[1].b, 2);
    }

    }

    #[with_checked_arithmetic]
    mod with_checked_arithmetic_tests {

        struct Test {
            a: i32,
            b: i32,
        }

        fn unchecked_add(a: i32, b: i32) -> i32 {
            a + b
        }

        #[test]
        fn test_checked_arithmetic_macro() {
            unchecked_add(1, 2);
        }

        #[test]
        #[should_panic]
        fn test_checked_arithmetic_macro_panic() {
            unchecked_add(i32::MAX, 1);
        }

        fn unchecked_add_hidden(a: i32, b: i32) -> i32 {
            let inner = |a: i32, b: i32| a + b;
            inner(a, b)
        }

        #[test]
        #[should_panic]
        fn test_checked_arithmetic_macro_panic_hidden() {
            unchecked_add_hidden(i32::MAX, 1);
        }

        fn unchecked_add_hidden_2(a: i32, b: i32) -> i32 {
            fn inner(a: i32, b: i32) -> i32 {
                a + b
            }
            inner(a, b)
        }

        #[test]
        #[should_panic]
        fn test_checked_arithmetic_macro_panic_hidden_2() {
            unchecked_add_hidden_2(i32::MAX, 1);
        }

        impl Test {
            fn add(&self) -> i32 {
                self.a + self.b
            }
        }

        #[test]
        #[should_panic]
        fn test_checked_arithmetic_impl() {
            let t = Test { a: 1, b: i32::MAX };
            t.add();
        }

        #[test]
        #[should_panic]
        fn test_macro_overflow() {
            #[allow(arithmetic_overflow)]
            fn f() {
                println!("{}", i32::MAX + 1);
            }

            f()
        }

        // Make sure that we still do addition correctly!
        #[test]
        fn test_non_overflow() {
            fn f() {
                assert_eq!(1i32 + 2i32, 3i32);
                assert_eq!(3i32 - 1i32, 2i32);
                assert_eq!(4i32 * 3i32, 12i32);
                assert_eq!(12i32 / 3i32, 4i32);
                assert_eq!(12i32 % 5i32, 2i32);

                let mut a = 1i32;
                a += 2i32;
                assert_eq!(a, 3i32);

                let mut a = 3i32;
                a -= 1i32;
                assert_eq!(a, 2i32);

                let mut a = 4i32;
                a *= 3i32;
                assert_eq!(a, 12i32);

                let mut a = 12i32;
                a /= 3i32;
                assert_eq!(a, 4i32);

                let mut a = 12i32;
                a %= 5i32;
                assert_eq!(a, 2i32);
            }

            f();
        }

        #[test]
        fn test_exprs_evaluated_once_right() {
            let mut called = false;
            let mut f = || {
                if called {
                    panic!("called twice");
                }
                called = true;
                1i32
            };

            assert_eq!(2i32 + f(), 3);
        }

        #[test]
        fn test_exprs_evaluated_once_left() {
            let mut called = false;
            let mut f = || {
                if called {
                    panic!("called twice");
                }
                called = true;
                1i32
            };

            assert_eq!(f() + 2i32, 3);
        }

        #[test]
        fn test_assign_op_evals_once() {
            struct Foo {
                a: i32,
                called: bool,
            }

            impl Foo {
                fn get_a_mut(&mut self) -> &mut i32 {
                    if self.called {
                        panic!("called twice");
                    }
                    let ret = &mut self.a;
                    self.called = true;
                    ret
                }
            }

            let mut foo = Foo {
                a: 1,
                called: false,
            };

            *foo.get_a_mut() += 2;
            assert_eq!(foo.a, 3);
        }

        #[test]
        fn test_more_macro_syntax() {
            struct Foo {
                a: i32,
                b: i32,
            }

            impl Foo {
                const BAR: i32 = 1;

                fn new(a: i32, b: i32) -> Foo {
                    Foo { a, b }
                }
            }

            fn new_foo(a: i32) -> Foo {
                Foo { a, b: 0 }
            }

            // verify that we translate the contents of macros correctly
            assert_eq!(Foo::BAR + 1, 2);
            assert_eq!(Foo::new(1, 2).b, 2);
            assert_eq!(new_foo(1).a, 1);

            let v = [Foo::new(1, 2), Foo::new(3, 2)];

            assert_eq!(v[0].a, 1);
            assert_eq!(v[1].b, 2);
        }
    }
}
