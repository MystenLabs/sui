// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
//

use std::cell::{Cell, RefCell};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

pub struct ScopedTimer;

thread_local! {
    static SCOPE_LOGGER: RefCell<ScopeLoggerState> = RefCell::new(ScopeLoggerState::new());
}

struct ScopeLoggerState {
    indent: AtomicUsize,
}

impl ScopeLoggerState {
    fn new() -> Self {
        Self {
            indent: AtomicUsize::new(0),
        }
    }

    fn increment_indent(&self) {
        //self.indent.fetch_add(1, Ordering::SeqCst);
    }

    fn decrement_indent(&self) {
        //self.indent.fetch_sub(1, Ordering::SeqCst);
    }

    fn print_indented(&self, file: &str, line: u32, message: &str) {
        let indent = self.indent.load(Ordering::SeqCst);
        tracing::trace!("{}{} [{file}:{line}]", "  ".repeat(indent), message);
    }
}

pub struct ScopedTimerGuard {
    file: &'static str,
    line: u32,
    name: &'static str,
    start_time: Cell<Option<Instant>>,
    elapsed: Cell<Duration>,
    count: Cell<Option<u32>>,
}

#[macro_export]
macro_rules! new_scoped_timer {
    ($name:expr) => {
        $crate::scoped_timer::ScopedTimer::new(file!(), line!(), $name)
    };
    ($name:expr, $count:expr) => {
        $crate::scoped_timer::ScopedTimer::new_with_count(file!(), line!(), $name, $count)
    };
}

impl ScopedTimerGuard {
    pub fn set_count(&self, count: usize) {
        self.count.set(Some(count as u32));
    }

    pub fn increment_count(&self, count: usize) {
        let cur = self.count.take().unwrap_or(0);
        self.count.set(Some(cur.saturating_add(count as u32)));
    }

    pub fn resume(&self) {
        assert!(self.start_time.get().is_none());
        self.start_time.set(Some(Instant::now()));
    }

    pub fn pause(&self) {
        // pausing multiple times is ok
        if let Some(start) = self.start_time.take() {
            let elapsed = start.elapsed();
            self.elapsed.set(self.elapsed.get() + elapsed);
        }
    }
}

impl ScopedTimer {
    pub fn new_with_count(
        file: &'static str,
        line: u32,
        name: &'static str,
        count: usize,
    ) -> ScopedTimerGuard {
        let ret = Self::new_impl(file, line, name);
        ret.set_count(count);
        ret
    }

    pub fn paused_with_count(
        file: &'static str,
        line: u32,
        name: &'static str,
        count: usize,
    ) -> ScopedTimerGuard {
        let ret = Self::new_impl(file, line, name);
        ret.start_time.take();
        ret.set_count(count);
        ret
    }

    pub fn paused(file: &'static str, line: u32, name: &'static str) -> ScopedTimerGuard {
        let ret = Self::new_impl(file, line, name);
        ret.start_time.take();
        ret
    }

    pub fn new(file: &'static str, line: u32, name: &'static str) -> ScopedTimerGuard {
        Self::new_impl(file, line, name)
    }

    fn new_impl(file: &'static str, line: u32, name: &'static str) -> ScopedTimerGuard {
        let guard = ScopedTimerGuard {
            file,
            line,
            name,
            start_time: Cell::new(Some(Instant::now())),
            elapsed: Cell::new(Duration::from_secs(0)),
            count: Cell::new(None),
        };
        SCOPE_LOGGER.with(|logger| {
            logger.borrow().increment_indent();
        });
        guard
    }
}

impl Drop for ScopedTimerGuard {
    fn drop(&mut self) {
        self.pause();
        let elapsed = self.elapsed.get();
        let count = self.count.take();
        SCOPE_LOGGER.with(|logger| {
            match count {
                Some(count) if count > 0 => {
                    logger.borrow().print_indented(self.file, self.line, &format!(
                        "Exiting scope: {} (elapsed time: {:?}, count: {:?}, time per element: {:?})",
                        self.name,
                        elapsed,
                        count,
                        elapsed / count
                    ));
                }
                _ => {
                    logger.borrow().print_indented(self.file, self.line, &format!(
                        "Exiting scope: {} (elapsed time: {:?})",
                        self.name, elapsed
                    ));
                }
            }
            logger.borrow().decrement_indent();
        });
    }
}
