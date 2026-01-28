// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tower middleware layer for dynamic RPC validation.
//!
//! This layer intercepts gRPC requests before they reach the handler,
//! extracts the request body, and validates it using the DynamicRpcValidator.
//! If validation fails, the request is rejected with a PERMISSION_DENIED status.

use bytes::Bytes;
use http::{Request, Response, StatusCode};
use http_body::Body as HttpBody;
use http_body_util::BodyExt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use sui_dynamic_rpc_validator::{DynamicRpcValidator, RpcMethod};
use tower::{Layer, Service};

/// Tower layer that adds dynamic RPC validation to a service.
#[derive(Clone)]
pub struct ValidationLayer {
    validator: Option<Arc<DynamicRpcValidator>>,
}

impl ValidationLayer {
    pub fn new(validator: Option<Arc<DynamicRpcValidator>>) -> Self {
        Self { validator }
    }
}

impl<S> Layer<S> for ValidationLayer {
    type Service = ValidationService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        ValidationService {
            inner,
            validator: self.validator.clone(),
        }
    }
}

/// Service wrapper that performs validation before forwarding requests.
#[derive(Clone)]
pub struct ValidationService<S> {
    inner: S,
    validator: Option<Arc<DynamicRpcValidator>>,
}

impl<S, ReqBody, RespBody> Service<Request<ReqBody>> for ValidationService<S>
where
    S: Service<Request<http_body_util::Full<Bytes>>, Response = Response<RespBody>>
        + Clone
        + Send
        + 'static,
    S::Future: Send,
    S::Error: Send,
    ReqBody: HttpBody<Data = Bytes> + Send + 'static,
    ReqBody::Error: std::fmt::Display,
    RespBody: Default,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        let validator = self.validator.clone();
        let mut inner = self.inner.clone();

        Box::pin(async move {
            // Collect body bytes first (needed for both validation and forwarding)
            let (parts, body) = request.into_parts();
            let collected = match body.collect().await {
                Ok(collected) => collected,
                Err(e) => {
                    tracing::warn!("Failed to collect request body: {}", e);
                    return Ok(rejection_response(
                        "Failed to read request body",
                        tonic::Code::Internal,
                    ));
                }
            };
            let body_bytes = collected.to_bytes();

            // If no validator configured, pass through directly
            let Some(validator) = validator else {
                let body = http_body_util::Full::new(body_bytes);
                let request = Request::from_parts(parts, body);
                return inner.call(request).await;
            };

            // Extract RPC method from path
            let path = parts.uri.path();
            let rpc_method = match parse_rpc_method(path) {
                Some(method) => method,
                None => {
                    // Unknown method, pass through (e.g., health checks)
                    let body = http_body_util::Full::new(body_bytes);
                    let request = Request::from_parts(parts, body);
                    return inner.call(request).await;
                }
            };

            // Skip the gRPC framing (5 bytes: 1 byte compressed flag + 4 bytes length)
            // to get the actual protobuf message
            let message_bytes = if body_bytes.len() > 5 {
                &body_bytes[5..]
            } else {
                &body_bytes[..]
            };

            // Validate the request
            if !validator.validate(rpc_method, message_bytes) {
                tracing::info!(
                    rpc_method = ?rpc_method,
                    "Request rejected by dynamic validator"
                );
                return Ok(rejection_response(
                    &format!(
                        "Request rejected by dynamic validator for RPC: {:?}",
                        rpc_method
                    ),
                    tonic::Code::PermissionDenied,
                ));
            }

            // Reconstruct the request with the collected body
            let body = http_body_util::Full::new(body_bytes);
            let request = Request::from_parts(parts, body);

            inner.call(request).await
        })
    }
}

/// Parse the gRPC method path to determine the RPC method type.
/// gRPC paths are in the format: /package.Service/Method
fn parse_rpc_method(path: &str) -> Option<RpcMethod> {
    // Expected format: /sui.validator.Validator/MethodName
    let method_name = path.rsplit('/').next()?;

    match method_name {
        "SubmitTransaction" => Some(RpcMethod::SubmitTransaction),
        "WaitForEffects" => Some(RpcMethod::WaitForEffects),
        "ObjectInfo" | "HandleObjectInfoRequest" => Some(RpcMethod::ObjectInfo),
        "TransactionInfo" | "HandleTransactionInfoRequest" => Some(RpcMethod::TransactionInfo),
        "Checkpoint" | "HandleCheckpointRequest" => Some(RpcMethod::Checkpoint),
        "CheckpointV2" | "HandleCheckpointRequestV2" => Some(RpcMethod::CheckpointV2),
        "GetSystemStateObject" | "HandleSystemStateRequest" => {
            Some(RpcMethod::GetSystemStateObject)
        }
        "ValidatorHealth" => Some(RpcMethod::ValidatorHealth),
        _ => None,
    }
}

/// Create a gRPC error response with the appropriate headers
fn rejection_response<B: Default>(message: &str, code: tonic::Code) -> Response<B> {
    // URL-encode the message for the grpc-message header
    let encoded_message: String = message
        .bytes()
        .flat_map(|b| {
            if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
                vec![b as char]
            } else {
                format!("%{:02X}", b).chars().collect()
            }
        })
        .collect();

    Response::builder()
        .status(StatusCode::OK) // gRPC uses 200 OK with grpc-status header
        .header("content-type", "application/grpc")
        .header("grpc-status", (code as i32).to_string())
        .header("grpc-message", encoded_message)
        .body(B::default())
        .unwrap_or_else(|_| Response::new(B::default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rpc_method() {
        assert_eq!(
            parse_rpc_method("/sui.validator.Validator/SubmitTransaction"),
            Some(RpcMethod::SubmitTransaction)
        );
        assert_eq!(
            parse_rpc_method("/sui.validator.Validator/WaitForEffects"),
            Some(RpcMethod::WaitForEffects)
        );
        assert_eq!(
            parse_rpc_method("/sui.validator.Validator/ObjectInfo"),
            Some(RpcMethod::ObjectInfo)
        );
        assert_eq!(
            parse_rpc_method("/grpc.health.v1.Health/Check"),
            None
        );
        assert_eq!(
            parse_rpc_method("/sui.validator.Validator/UnknownMethod"),
            None
        );
    }
}
