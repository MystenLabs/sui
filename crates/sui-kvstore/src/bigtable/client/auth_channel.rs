// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::RwLock;
use std::task::Context;
use std::task::Poll;

use gcp_auth::Token;
use gcp_auth::TokenProvider;
use http::HeaderValue;
use http::Request;
use http::Response;
use tonic::body::Body;
use tonic::codegen::Service;

const FEATURE_FLAGS: &str = "CAEoAQ==";

/// Auth middleware that injects credentials onto any inner `Service`.
#[derive(Clone)]
pub(crate) struct AuthChannel<S> {
    inner: S,
    policy: String,
    token_provider: Option<Arc<dyn TokenProvider>>,
    token: Arc<RwLock<Option<Arc<Token>>>>,
}

impl<S> AuthChannel<S> {
    pub(crate) fn new(
        inner: S,
        policy: String,
        token_provider: Option<Arc<dyn TokenProvider>>,
    ) -> Self {
        Self {
            inner,
            policy,
            token_provider,
            token: Arc::new(RwLock::new(None)),
        }
    }
}

impl<S> Service<Request<Body>> for AuthChannel<S>
where
    S: Service<Request<Body>, Response = Response<Body>> + Clone + Send + 'static,
    S::Error: Into<Box<dyn std::error::Error + Send + Sync>>,
    S::Future: Send,
{
    type Response = Response<Body>;
    type Error = Box<dyn std::error::Error + Send + Sync>;
    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(Into::into)
    }

    fn call(&mut self, mut request: Request<Body>) -> Self::Future {
        let cloned_token = self.token.clone();
        let policy = self.policy.clone();
        let token_provider = self.token_provider.clone();

        let mut auth_token = None;
        if token_provider.is_some() {
            let guard = self.token.read().expect("failed to acquire a read lock");
            if let Some(token) = &*guard
                && !token.has_expired()
            {
                auth_token = Some(token.clone());
            }
        }

        // Take the poll_ready'd inner service, replace with a fresh clone.
        let cloned = self.inner.clone();
        let mut ready_inner = std::mem::replace(&mut self.inner, cloned);

        Box::pin(async move {
            if let Some(ref provider) = token_provider {
                let token = match auth_token {
                    None => {
                        let new_token = provider.token(&[policy.as_ref()]).await?;
                        let mut guard = cloned_token.write().unwrap();
                        *guard = Some(new_token.clone());
                        new_token
                    }
                    Some(token) => token,
                };
                let token_string = token.as_str().parse::<String>()?;
                let header =
                    HeaderValue::from_str(format!("Bearer {}", token_string.as_str()).as_str())?;
                request.headers_mut().insert("authorization", header);
            }
            // Advertise client-supported BigTable features via base64 FeatureFlags:
            //   field 1 (reverse_scans)          -> tag byte 0x08, value 0x01
            //   field 5 (mutate_rows_rate_limit2) -> tag byte 0x28, value 0x01
            // "CAEoAQ==" decodes to [0x08, 0x01, 0x28, 0x01]. Field 5 opts into
            // MutateRows server-side flow control WITH partial retries enabled.
            // Field 3 (mutate_rows_rate_limit) is deliberately omitted: it disables
            // partial retries, which write_entries relies on (PartialWriteError).
            let header = HeaderValue::from_static(FEATURE_FLAGS);
            request.headers_mut().insert("bigtable-features", header);

            ready_inner.call(request).await.map_err(Into::into)
        })
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use base64::engine::general_purpose::STANDARD;

    use super::FEATURE_FLAGS;

    #[test]
    fn feature_flags_header_decodes_to_reverse_scan_and_rate_limit2() {
        let decoded = STANDARD.decode(FEATURE_FLAGS).unwrap();

        // FeatureFlags bytes:
        //   0x08 = tag for field 1 (reverse_scans), 0x01 = true.
        //   0x28 = tag for field 5 (mutate_rows_rate_limit2), 0x01 = true.
        // Field 3 (mutate_rows_rate_limit) is intentionally not set because it
        // disables partial retries.
        assert_eq!(decoded, [0x08, 0x01, 0x28, 0x01]);
    }
}
