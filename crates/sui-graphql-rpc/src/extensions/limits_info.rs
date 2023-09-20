// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextRequest, NextValidation},
    value, Response, ServerError, ValidationResult,
};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::ServiceConfig;

pub struct LimitsInfo;

impl ExtensionFactory for LimitsInfo {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(LimitsInfoExtension::default())
    }
}

#[derive(Default)]
struct LimitsInfoExtension {
    validation_result: Mutex<Option<ValidationResult>>,
}

#[async_trait::async_trait]
impl Extension for LimitsInfoExtension {
    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        let cfg = ctx
            .data::<ServiceConfig>()
            .expect("No service config provided in schema data");
        let mut resp = next.run(ctx).await;
        let validation_result = self.validation_result.lock().await.take();
        if let Some(validation_result) = validation_result {
            resp = resp.extension(
                "usage", // need better name for this
                value! ({
                    "nodes": validation_result.complexity,
                    "depth": validation_result.depth,
                }),
            );
            resp = resp.extension(
                "limits",
                value! ({
                    "maxNodes": cfg.limits.max_query_nodes,
                    "maxDepth": cfg.limits.max_query_depth,
                }),
            );
        }
        resp
    }

    async fn validation(
        &self,
        ctx: &ExtensionContext<'_>,
        next: NextValidation<'_>,
    ) -> Result<ValidationResult, Vec<ServerError>> {
        let res = next.run(ctx).await?;
        *self.validation_result.lock().await = Some(res);
        Ok(res)
    }
}
