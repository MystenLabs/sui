// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::RwLock;
use std::task::Context;
use std::task::Poll;

use base64::Engine as _;
use gcp_auth::Token;
use gcp_auth::TokenProvider;
use http::HeaderValue;
use http::Request;
use http::Response;
use prost::Message as _;
use tonic::body::Body;
use tonic::codegen::Service;

use crate::bigtable::proto::bigtable::v2::FeatureFlags;

/// Websafe-base64 [`FeatureFlags`] advertised to Bigtable in the
/// `bigtable-features` request metadata. Reverse scans are always advertised.
/// Batch write flow control additionally advertises both MutateRows rate-limit
/// flags so that partial retries remain supported.
pub(crate) fn bigtable_features_header(batch_write_flow_control: bool) -> HeaderValue {
    let feature_flags = FeatureFlags {
        reverse_scans: true,
        mutate_rows_rate_limit: batch_write_flow_control,
        mutate_rows_rate_limit2: batch_write_flow_control,
        ..Default::default()
    };
    let encoded = base64::engine::general_purpose::URL_SAFE.encode(feature_flags.encode_to_vec());
    HeaderValue::from_str(&encoded).expect("base64 is always a valid header value")
}

/// Auth middleware that injects credentials onto any inner `Service`.
#[derive(Clone)]
pub(crate) struct AuthChannel<S> {
    inner: S,
    policy: String,
    token_provider: Option<Arc<dyn TokenProvider>>,
    features_header: HeaderValue,
    token: Arc<RwLock<Option<Arc<Token>>>>,
}

impl<S> AuthChannel<S> {
    pub(crate) fn new(
        inner: S,
        policy: String,
        token_provider: Option<Arc<dyn TokenProvider>>,
        features_header: HeaderValue,
    ) -> Self {
        Self {
            inner,
            policy,
            token_provider,
            features_header,
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
        let features_header = self.features_header.clone();

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
            request
                .headers_mut()
                .insert("bigtable-features", features_header);

            ready_inner.call(request).await.map_err(Into::into)
        })
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine as _;
    use prost::Message as _;

    use super::*;

    #[test]
    fn default_features_header_is_byte_compatible() {
        assert_eq!(
            bigtable_features_header(false),
            HeaderValue::from_static("CAE="),
        );
    }

    #[test]
    fn flow_control_features_header_round_trips() {
        let header = bigtable_features_header(true);
        let encoded = base64::engine::general_purpose::URL_SAFE
            .decode(header.as_bytes())
            .expect("features header should be valid websafe base64");
        let feature_flags =
            FeatureFlags::decode(encoded.as_slice()).expect("features header should be valid");

        assert_eq!(
            feature_flags,
            FeatureFlags {
                reverse_scans: true,
                mutate_rows_rate_limit: true,
                mutate_rows_rate_limit2: true,
                ..Default::default()
            },
        );
    }
}
