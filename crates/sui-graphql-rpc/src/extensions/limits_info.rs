// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextRequest, NextValidation},
    value, Response, ServerError, ValidationResult,
};
use axum::{
    headers,
    http::{HeaderName, HeaderValue},
};
use std::sync::Arc;
use tokio::sync::Mutex;

static LIMITS_HEADER: HeaderName = HeaderName::from_static("x-sui-rpc-show-usage");

/// Only display usage information if this header was in the request.
pub(crate) struct ShowUsage;

pub(crate) struct LimitsInfo;

#[derive(Default)]
struct LimitsInfoExtension {
    validation_result: Mutex<Option<ValidationResult>>,
}

impl headers::Header for ShowUsage {
    fn name() -> &'static HeaderName {
        &LIMITS_HEADER
    }

    fn decode<'i, I>(_: &mut I) -> Result<Self, headers::Error>
    where
        I: Iterator<Item = &'i HeaderValue>,
    {
        Ok(ShowUsage)
    }

    fn encode<E: Extend<HeaderValue>>(&self, _: &mut E) {
        unimplemented!()
    }
}

impl ExtensionFactory for LimitsInfo {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(LimitsInfoExtension::default())
    }
}

#[async_trait::async_trait]
impl Extension for LimitsInfoExtension {
    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        let resp = next.run(ctx).await;
        let validation_result = self.validation_result.lock().await.take();
        if let Some(validation_result) = validation_result {
            resp.extension(
                "usage",
                value! ({
                    "nodes": validation_result.complexity,
                    "depth": validation_result.depth,
                }),
            )
        } else {
            resp
        }
    }

    async fn validation(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextValidation<'_>,
    ) -> Result<ValidationResult, Vec<ServerError>> {
        let res = next.run(ctx).await?;
        if ctx.data_opt::<ShowUsage>().is_some() {
            *self.validation_result.lock().await = Some(res);
        }
        Ok(res)
    }
}
