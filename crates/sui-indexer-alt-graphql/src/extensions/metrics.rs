// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use async_graphql::{
    extensions::{Extension, ExtensionContext, ExtensionFactory, NextRequest},
    Response,
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
}
