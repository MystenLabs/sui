// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::in_test_configuration;
use once_cell::sync::Lazy;

#[macro_export]
macro_rules! fatal {
    ($msg:literal $(, $arg:expr)*) => {{
        if $crate::in_antithesis() {
            let full_msg = format!($msg $(, $arg)*);
            let json = $crate::logging::json!({ "message": full_msg });
            $crate::logging::assert_unreachable_antithesis!($msg, &json);
        }
        tracing::error!(fatal = true, $msg $(, $arg)*);
        panic!($msg $(, $arg)*);
    }};
}

pub use antithesis_sdk::assert_reachable as assert_reachable_antithesis;
pub use antithesis_sdk::assert_sometimes as assert_sometimes_antithesis;
pub use antithesis_sdk::assert_unreachable as assert_unreachable_antithesis;

pub use serde_json::json;

#[inline(always)]
pub fn crash_on_debug() -> bool {
    static CRASH_ON_DEBUG: Lazy<bool> = Lazy::new(|| {
        in_test_configuration() || std::env::var("SUI_ENABLE_DEBUG_ASSERTIONS").is_ok()
    });

    *CRASH_ON_DEBUG
}

#[cfg(msim)]
pub mod intercept_debug_fatal {
    use std::sync::{Arc, Mutex};

    #[derive(Clone)]
    pub struct DebugFatalCallback {
        pub pattern: String,
        pub callback: Arc<dyn Fn() + Send + Sync>,
    }

    thread_local! {
        static INTERCEPT_DEBUG_FATAL: Mutex<Option<DebugFatalCallback>> = Mutex::new(None);
    }

    pub fn register_callback(message: &str, f: impl Fn() + Send + Sync + 'static) {
        INTERCEPT_DEBUG_FATAL.with(|m| {
            *m.lock().unwrap() = Some(DebugFatalCallback {
                pattern: message.to_string(),
                callback: Arc::new(f),
            });
        });
    }

    pub fn get_callback() -> Option<DebugFatalCallback> {
        INTERCEPT_DEBUG_FATAL.with(|m| m.lock().unwrap().clone())
    }
}

#[macro_export]
macro_rules! register_debug_fatal_handler {
    ($message:literal, $f:expr) => {
        #[cfg(msim)]
        $crate::logging::intercept_debug_fatal::register_callback($message, $f);

        #[cfg(not(msim))]
        {
            // silence unused variable warnings from the body of the callback
            let _ = $f;
        }
    };
}

#[macro_export]
macro_rules! debug_fatal {
    //($msg:literal $(, $arg:expr)* $(,)?)
    ($msg:literal $(, $arg:expr)*) => {{
        loop {
            #[cfg(msim)]
            {
                if let Some(cb) = $crate::logging::intercept_debug_fatal::get_callback() {
                    tracing::error!($msg $(, $arg)*);
                    let msg = format!($msg $(, $arg)*);
                    if msg.contains(&cb.pattern) {
                        (cb.callback)();
                    }
                    break;
                }
            }

            // In antithesis, rather than crashing, we will use the assert_unreachable_antithesis
            // macro to catch the signal that something has gone wrong.
            if !$crate::in_antithesis() && $crate::logging::crash_on_debug() {
                $crate::fatal!($msg $(, $arg)*);
            } else {
                let stacktrace = std::backtrace::Backtrace::capture();
                tracing::error!(debug_fatal = true, stacktrace = ?stacktrace, $msg $(, $arg)*);
                let location = concat!(file!(), ':', line!());
                if let Some(metrics) = mysten_metrics::get_metrics() {
                    metrics.system_invariant_violations.with_label_values(&[location]).inc();
                }
                if $crate::in_antithesis() {
                    // antithesis requires a literal for first argument. pass the formatted argument
                    // as a string.
                    let full_msg = format!($msg $(, $arg)*);
                    let json = $crate::logging::json!({ "message": full_msg });
                    $crate::logging::assert_unreachable_antithesis!($msg, &json);
                }
            }
            break;
        }
    }};
}

#[macro_export]
macro_rules! assert_reachable {
    () => {
        $crate::logging::assert_reachable!("");
    };
    ($message:literal) => {{
        // calling in to antithesis sdk breaks determinisim in simtests (on linux only)
        if !cfg!(msim) {
            $crate::logging::assert_reachable_antithesis!($message);
        }
    }};
}

#[macro_export]
macro_rules! assert_sometimes {
    ($expr:expr, $message:literal) => {{
        // calling in to antithesis sdk breaks determinisim in simtests (on linux only)
        if !cfg!(msim) {
            $crate::logging::assert_sometimes_antithesis!($expr, $message);
        } else {
            // evaluate the expression in case it has side effects
            let _ = $expr;
        }
    }};
}

use futures::{pin_mut, FutureExt, Stream};
use std::io::Write;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio::io::{AsyncRead, AsyncReadExt};

pub struct StructuredLog<T, W> {
    sender: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    _phantom: PhantomData<T>,
    writer_handle: std::thread::JoinHandle<W>,
}

impl<T: std::marker::Sized + serde::Serialize, W: 'static + Write + Send> StructuredLog<T, W> {
    pub fn new(mut writer: W) -> Self {
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();

        let writer_handle = std::thread::spawn(move || {
            while let Some(bytes) = receiver.blocking_recv() {
                writer
                    .write_all(&(bytes.len() as u32).to_le_bytes())
                    .unwrap();
                writer.write_all(&bytes).unwrap();
                writer.flush().unwrap();
            }
            writer
        });

        Self {
            sender,
            _phantom: PhantomData,
            writer_handle,
        }
    }

    pub fn into_writer(self) -> W {
        let Self { writer_handle, .. } = self;
        writer_handle.join().unwrap()
    }

    pub fn write(&mut self, record: &T) -> std::io::Result<()> {
        let bytes = bcs::to_bytes(record)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

        self.sender
            .send(bytes)
            .expect("Writer thread has terminated");

        Ok(())
    }
}

pub struct StructuredLogReader<T, R> {
    reader: R,
    _phantom: PhantomData<T>,
}

impl<T: std::marker::Sized, R: AsyncRead> StructuredLogReader<T, R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            _phantom: PhantomData,
        }
    }
}

impl<
        T: serde::de::DeserializeOwned + std::marker::Sized + std::marker::Unpin,
        R: AsyncRead + std::marker::Unpin,
    > Stream for StructuredLogReader<T, R>
{
    type Item = std::io::Result<T>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        // Read length prefix
        let mut len_buf = [0u8; 4];
        let read_len = this.reader.read_exact(&mut len_buf);
        pin_mut!(read_len);

        match read_len.poll_unpin(cx) {
            Poll::Ready(Ok(_)) => {
                let len = u32::from_le_bytes(len_buf) as usize;
                let mut data = vec![0u8; len];

                // Read actual data
                let read_data = this.reader.read_exact(&mut data);
                pin_mut!(read_data);

                match read_data.poll_unpin(cx) {
                    Poll::Ready(Ok(_)) => {
                        let parsed = bcs::from_bytes::<T>(&data)
                            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e));
                        Poll::Ready(Some(parsed))
                    }
                    Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
                    Poll::Pending => Poll::Pending,
                }
            }
            Poll::Ready(Err(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                Poll::Ready(None)
            }
            Poll::Ready(Err(e)) => Poll::Ready(Some(Err(e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[should_panic]
    fn test_fatal() {
        fatal!("This is a fatal error");
    }

    #[test]
    #[should_panic]
    fn test_debug_fatal() {
        if cfg!(debug_assertions) {
            debug_fatal!("This is a debug fatal error");
        } else {
            // pass in release mode as well
            fatal!("This is a fatal error");
        }
    }

    #[cfg(not(debug_assertions))]
    #[test]
    fn test_debug_fatal_release_mode() {
        debug_fatal!("This is a debug fatal error");
    }

    #[test]
    fn test_assert_sometimes_side_effects() {
        let mut x = 0;

        let mut inc = || {
            x += 1;
            true
        };

        assert_sometimes!(inc(), "");
        assert_eq!(x, 1);
    }

    use futures::StreamExt;
    use serde::{Deserialize, Serialize};
    use std::io::Cursor;

    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestStruct {
        field1: String,
        field2: i32,
    }

    #[tokio::test]
    async fn test_structured_log_writer_reader() {
        let mut writer = StructuredLog::new(Vec::new());

        // Test writing multiple records
        let test_data = vec![
            TestStruct {
                field1: "test1".to_string(),
                field2: 42,
            },
            TestStruct {
                field1: "test2adfadf".to_string(),
                field2: 100,
            },
        ];

        for data in &test_data {
            writer.write(data).unwrap();
        }

        // Create reader from written data
        let cursor = Cursor::new(writer.into_writer());
        let mut reader = StructuredLogReader::<TestStruct, _>::new(cursor);

        // Test reading records
        let mut read_data = Vec::new();
        while let Some(result) = reader.next().await {
            read_data.push(result.unwrap());
        }

        assert_eq!(test_data, read_data);
    }

    #[tokio::test]
    async fn test_structured_log_empty() {
        let writer = StructuredLog::<TestStruct, Vec<u8>>::new(Vec::new());

        // Create reader from empty buffer
        let cursor = Cursor::new(writer.into_writer());
        let mut reader = StructuredLogReader::<TestStruct, _>::new(cursor);

        // Should return None for empty buffer
        assert!(reader.next().await.is_none());
    }
}
