// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Logs every response that ends in a server-side gRPC error, with its method, code and message.
//!
//! The status is read where the client reads it: from the response headers of a trailers-only
//! response (how a handler's `Err` is sent before any message), and from the trailers at the end of
//! the body (how a streaming handler reports an error after its first message, which
//! `rpc_requests` counts as `ok` because it only looks at the headers). Codes the client caused or
//! chose (`Cancelled`, `NotFound`, `InvalidArgument`, ...) are not logged.
//!
//! Lines are rate-limited per process to [`MAX_LINES_PER_SECOND`]. A line dropped by the limit is
//! counted in the `suppressed` field of the next line that is logged, so counting lines plus
//! `suppressed` still gives the number of failed requests.

use std::sync::LazyLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use std::time::Instant;

use http::HeaderMap;
use http::HeaderName;
use http::request;
use http::response;
use sui_http::middleware::callback::MakeCallbackHandler;
use sui_http::middleware::callback::ResponseHandler;
use tonic::Code;
use tonic::Status;

/// Error lines logged per second by one process before further failures are only counted.
const MAX_LINES_PER_SECOND: u64 = 20;

/// Longest message logged; longer messages are cut at a character boundary.
const MAX_MESSAGE_BYTES: usize = 1024;

const GRPC_STATUS: HeaderName = HeaderName::from_static("grpc-status");

static LOG_BUDGET: LogBudget = LogBudget::new(MAX_LINES_PER_SECOND);
static PROCESS_START: LazyLock<Instant> = LazyLock::new(Instant::now);

/// Builds one [`ServerErrorLog`] per request, for `sui_http`'s `CallbackLayer`.
#[derive(Clone, Copy, Default)]
pub(crate) struct MakeServerErrorLog;

impl MakeCallbackHandler for MakeServerErrorLog {
    type RequestHandler = ();
    type ResponseHandler = ServerErrorLog;

    fn make_handler(&self, request: &request::Parts) -> ((), ServerErrorLog) {
        let handler = ServerErrorLog {
            uri: request.uri.clone(),
            start: Instant::now(),
            messages_sent: 0,
        };
        ((), handler)
    }
}

/// Watches one response and logs it if it ends in a server-side error.
pub(crate) struct ServerErrorLog {
    /// The request URI; its path is the gRPC method.
    uri: http::Uri,
    start: Instant,
    /// Response body data frames sent before the response ended.
    messages_sent: u64,
}

impl ServerErrorLog {
    fn log(&self, ended_in: &'static str, code: Code, message: &str) {
        let Some(suppressed) = LOG_BUDGET.take(PROCESS_START.elapsed().as_secs()) else {
            return;
        };
        tracing::warn!(
            method = self.uri.path(),
            ?code,
            ended_in,
            messages_sent = self.messages_sent,
            elapsed_ms = self.start.elapsed().as_millis() as u64,
            suppressed,
            error = &message[..message.floor_char_boundary(MAX_MESSAGE_BYTES)],
            "request failed with a server error",
        );
    }
}

impl ResponseHandler for ServerErrorLog {
    fn on_response(&mut self, response: &response::Parts) {
        if let Some(status) = server_error(&response.headers) {
            self.log("headers", status.code(), status.message());
        }
    }

    fn on_service_error<E>(&mut self, error: &E)
    where
        E: std::fmt::Display + 'static,
    {
        self.log("service error", Code::Unknown, &error.to_string());
    }

    fn on_body_chunk<B>(&mut self, _chunk: &B)
    where
        B: bytes::Buf,
    {
        self.messages_sent += 1;
    }

    fn on_end_of_stream(&mut self, trailers: Option<&HeaderMap>) {
        if let Some(status) = trailers.and_then(server_error) {
            self.log("trailers", status.code(), status.message());
        }
    }

    fn on_body_error<E>(&mut self, error: &E)
    where
        E: std::fmt::Display + 'static,
    {
        self.log("body error", Code::Unknown, &error.to_string());
    }
}

/// The status in `headers` (response headers or trailers), if it is a server-side error.
fn server_error(headers: &HeaderMap) -> Option<Status> {
    let code = Code::from_bytes(headers.get(GRPC_STATUS)?.as_bytes());
    if !is_server_error(code) {
        return None;
    }
    Status::from_header_map(headers)
}

/// Whether `code` reports a failure of the server rather than of the request or the client.
fn is_server_error(code: Code) -> bool {
    matches!(
        code,
        Code::Unknown
            | Code::DeadlineExceeded
            | Code::ResourceExhausted
            | Code::Aborted
            | Code::Internal
            | Code::Unavailable
            | Code::DataLoss
    )
}

/// A per-second allowance of log lines, shared by every request in the process.
struct LogBudget {
    max_per_second: u64,
    /// The second (since process start) that `taken` counts lines for.
    second: AtomicU64,
    taken: AtomicU64,
    /// Lines refused since a line was last allowed.
    suppressed: AtomicU64,
}

impl LogBudget {
    const fn new(max_per_second: u64) -> Self {
        Self {
            max_per_second,
            second: AtomicU64::new(0),
            taken: AtomicU64::new(0),
            suppressed: AtomicU64::new(0),
        }
    }

    /// Takes one line from the allowance for `second`. Returns the number of lines refused since
    /// the last allowed one, or `None` if this line is refused too.
    fn take(&self, second: u64) -> Option<u64> {
        let current = self.second.load(Ordering::Relaxed);
        if second > current
            && self
                .second
                .compare_exchange(current, second, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            self.taken.store(0, Ordering::Relaxed);
        }
        if self.taken.fetch_add(1, Ordering::Relaxed) < self.max_per_second {
            Some(self.suppressed.swap(0, Ordering::Relaxed))
        } else {
            self.suppressed.fetch_add(1, Ordering::Relaxed);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(status: &str, message: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(GRPC_STATUS, status.parse().unwrap());
        if let Some(message) = message {
            headers.insert("grpc-message", message.parse().unwrap());
        }
        headers
    }

    #[test]
    fn server_errors_are_reported_with_their_decoded_message() {
        let status = server_error(&headers("13", Some("BigTable%20read%20failed"))).unwrap();
        assert_eq!(status.code(), Code::Internal);
        assert_eq!(status.message(), "BigTable read failed");

        let status = server_error(&headers("14", None)).unwrap();
        assert_eq!(status.code(), Code::Unavailable);
    }

    #[test]
    fn success_and_client_caused_codes_are_not_reported() {
        // Ok, Cancelled, InvalidArgument, NotFound, FailedPrecondition, OutOfRange.
        for code in ["0", "1", "3", "5", "9", "11"] {
            assert!(
                server_error(&headers(code, Some("x"))).is_none(),
                "code {code}"
            );
        }
        assert!(server_error(&HeaderMap::new()).is_none());
    }

    #[test]
    fn budget_refuses_lines_past_the_limit_and_reports_them_on_the_next_second() {
        let budget = LogBudget::new(2);
        assert_eq!(budget.take(5), Some(0));
        assert_eq!(budget.take(5), Some(0));
        assert_eq!(budget.take(5), None);
        assert_eq!(budget.take(5), None);
        // A new second refills the allowance; the first line carries the refused count.
        assert_eq!(budget.take(6), Some(2));
        assert_eq!(budget.take(6), Some(0));
        assert_eq!(budget.take(6), None);
    }
}
