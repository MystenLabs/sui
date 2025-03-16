// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::{
    extensions::{
        Extension, ExtensionContext, ExtensionFactory, NextRequest, NextResolve, ResolveInfo,
    },
    Response, ServerResult, Value,
};

use crate::metrics::RpcMetrics;

pub(crate) struct Metrics(pub Arc<RpcMetrics>);

struct MetricsExt(pub Arc<RpcMetrics>);

impl ExtensionFactory for Metrics {
    fn create(&self) -> Arc<dyn Extension> {
        Arc::new(MetricsExt(self.0.clone()))
    }
}

#[async_trait::async_trait]
impl Extension for MetricsExt {
    /// Track query-wide metrics
    async fn request(&self, ctx: &ExtensionContext<'_>, next: NextRequest<'_>) -> Response {
        self.0.queries_received.inc();
        self.0.queries_in_flight.inc();

        let _guard = self.0.query_latency.start_timer();
        let response = next.run(ctx).await;

        self.0.queries_in_flight.dec();
        if response.is_ok() {
            self.0.queries_succeeded.inc();
        } else {
            self.0.queries_failed.inc();
        }

        response
    }

    /// Track metrics per field
    async fn resolve(
        &self,
        ctx: &ExtensionContext<'_>,
        info: ResolveInfo<'_>,
        next: NextResolve<'_>,
    ) -> ServerResult<Option<Value>> {
        let labels = &[info.parent_type, info.name];
        self.0.fields_received.with_label_values(labels).inc();

        let result = next.run(ctx, info).await;
        if result.is_ok() {
            self.0.fields_succeeded.with_label_values(labels).inc();
        } else {
            self.0.fields_failed.with_label_values(labels).inc();
        }

        result
    }
}
