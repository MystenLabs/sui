// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Per-subscriber delivery throttle for subscriptions. See [`Throttle`].

use std::time::Duration;

use async_graphql::Response;
use async_graphql::Value;
use futures::Stream;
use futures::StreamExt;
use prometheus::Histogram;

use crate::extensions::query_limits::QueryDepth;

/// Fixed per-level surcharge added to a payload's cost, approximating the sequential DB round trips a
/// payload makes (roughly one per level of query depth), which the output-node count alone does not
/// capture. The weight follows Shopify's rate-limiting model, which prices a fetch at two points plus
/// the objects it returns; the returned objects are already counted as output nodes, so each level of
/// depth carries only the fixed two-point part.
///
/// Ref: <https://shopify.engineering/rate-limiting-graphql-apis-calculating-query-complexity>
const DEPTH_NODE_COST: u32 = 2;

/// Paces subscription delivery to a sustained rate of `nodes_per_second` output nodes per second.
///
/// A payload's cost is normalized to output-node-equivalents, then held for `cost / rate` seconds
/// before delivery:
///
/// ```text
/// cost  = output_nodes + query_depth * DEPTH_NODE_COST
/// delay = cost / nodes_per_second
/// ```
///
/// So a 10-node payload at query depth 5 costs `10 + 5*2 = 20`, and at 40 nodes/second is held
/// `20 / 40 = 0.5s`. A rate of `0` disables pacing, and payloads are never dropped or reordered.
#[derive(Clone)]
pub(crate) struct Throttle {
    nodes_per_second: u32,
    /// Observes each payload's pacing delay in seconds (including zero when the budget is not
    /// binding), for the `throttle_delay` metric.
    delay_metric: Histogram,
}

impl Throttle {
    pub(crate) fn new(nodes_per_second: u32, delay_metric: Histogram) -> Self {
        Self {
            nodes_per_second,
            delay_metric,
        }
    }

    /// Pace `stream`, delivering each payload immediately then pausing before the next for its delay.
    pub(crate) fn wrap<S>(self, stream: S, query_depth: QueryDepth) -> impl Stream<Item = Response>
    where
        S: Stream<Item = Response>,
    {
        async_stream::stream! {
            let mut stream = std::pin::pin!(stream);
            while let Some(response) = stream.next().await {
                // Deliver immediately, then pause before pulling the next payload, which also
                // backpressures its resolution. Depth is constant but only known once validation has
                // run, so read the slot here.
                let delay = self.calculate_delay(&response.data, query_depth.get());
                self.delay_metric.observe(delay.as_secs_f64());
                yield response;
                if !delay.is_zero() {
                    tokio::time::sleep(delay).await;
                }
            }
        }
    }

    /// The delay for one payload: its cost spread over `cost / rate` seconds, or `Duration::ZERO`
    /// when disabled.
    pub(crate) fn calculate_delay(&self, payload: &Value, query_depth: u32) -> Duration {
        if self.nodes_per_second == 0 {
            return Duration::ZERO;
        }
        let cost = payload_cost(payload, query_depth);
        Duration::from_secs_f64(cost as f64 / self.nodes_per_second as f64)
    }
}

/// The delivery cost of a payload in output-node-equivalents: its actual output nodes plus a surcharge
/// for the subscription's query depth (constant across the subscription's payloads).
fn payload_cost(value: &Value, query_depth: u32) -> u32 {
    count_output_nodes(value).saturating_add(query_depth.saturating_mul(DEPTH_NODE_COST))
}

/// Count the output nodes in a resolved payload: one per object, one per list, and one per scalar
/// leaf, summed over the tree.
fn count_output_nodes(value: &Value) -> u32 {
    match value {
        Value::Object(fields) => fields
            .values()
            .map(count_output_nodes)
            .fold(1, u32::saturating_add),
        Value::List(items) => items
            .iter()
            .map(count_output_nodes)
            .fold(1, u32::saturating_add),
        _ => 1,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::collections::BTreeSet;

    use async_graphql::EmptyMutation;
    use async_graphql::Object;
    use async_graphql::Request;
    use async_graphql::Schema;
    use async_graphql::SimpleObject;
    use async_graphql::Subscription;
    use async_graphql::value;

    use crate::extensions::query_limits::QueryLimitsChecker;
    use crate::extensions::query_limits::QueryLimitsConfig;
    use crate::metrics::RpcMetrics;
    use crate::pagination::PageLimits;
    use crate::pagination::PaginationConfig;

    use super::*;

    fn throttle(nodes_per_second: u32) -> Throttle {
        Throttle::new(nodes_per_second, test_histogram())
    }

    fn test_histogram() -> Histogram {
        Histogram::with_opts(prometheus::HistogramOpts::new("test", "test")).unwrap()
    }

    #[derive(SimpleObject)]
    struct TestNestedPayload {
        left: i32,
        middle: i32,
        right: i32,
    }

    #[derive(SimpleObject)]
    struct TestPayload {
        leaf: i32,
        nested: TestNestedPayload,
    }

    struct TestQuery;

    #[Object]
    impl TestQuery {
        async fn health(&self) -> bool {
            true
        }
    }

    struct TestSubscription;

    #[Subscription]
    impl TestSubscription {
        async fn payloads(&self) -> impl Stream<Item = TestPayload> {
            futures::stream::iter((0..2).map(|_| TestPayload {
                leaf: 1,
                nested: TestNestedPayload {
                    left: 2,
                    middle: 3,
                    right: 4,
                },
            }))
        }
    }

    fn test_query_limits() -> QueryLimitsConfig {
        QueryLimitsConfig {
            max_output_nodes: 100,
            max_query_nodes: 100,
            max_query_depth: 10,
            max_query_payload_size: 100,
            max_tx_payload_size: 100,
            tx_payload_args: BTreeSet::new(),
        }
    }

    fn test_schema() -> Schema<TestQuery, EmptyMutation, TestSubscription> {
        let registry = prometheus::Registry::new();
        let metrics = RpcMetrics::new(&registry);

        Schema::build(TestQuery, EmptyMutation, TestSubscription)
            .extension(QueryLimitsChecker::new(test_query_limits(), metrics))
            .data(PaginationConfig::new(
                10,
                PageLimits {
                    default: 10,
                    max: 10,
                },
                BTreeMap::new(),
            ))
            .finish()
    }

    #[test]
    fn counts_scalars_and_objects() {
        // A single scalar.
        assert_eq!(count_output_nodes(&value!(1)), 1);
        // An object (1) with two scalar fields (1 each).
        assert_eq!(count_output_nodes(&value!({ "a": 1, "b": 2 })), 3);
        // Nesting: outer object (1) + inner object (1 + two scalars = 3).
        assert_eq!(count_output_nodes(&value!({ "a": { "b": 1, "c": 2 } })), 4);
    }

    #[test]
    fn counts_lists_and_elements() {
        // Object (1) + list (1) + three scalar elements (1 each).
        assert_eq!(count_output_nodes(&value!({ "items": [1, 2, 3] })), 5);
        // Object (1) + list (1) + two element objects (1 + one scalar each = 2).
        assert_eq!(
            count_output_nodes(&value!({ "nodes": [{ "x": 1 }, { "x": 2 }] })),
            6
        );
    }

    #[test]
    fn payload_cost_adds_depth_surcharge() {
        let payload = value!({ "a": 1, "b": 2 }); // 3 output nodes
        assert_eq!(payload_cost(&payload, 0), 3);
        assert_eq!(payload_cost(&payload, 4), 3 + 4 * DEPTH_NODE_COST);
    }

    #[test]
    fn zero_rate_disables_pacing() {
        let payload = value!({ "a": 1, "b": 2 });
        assert_eq!(throttle(0).calculate_delay(&payload, 100), Duration::ZERO);
    }

    #[test]
    fn delay_is_cost_over_rate() {
        let payload = value!({ "a": 1, "b": 2, "c": 3, "d": 4 }); // 5 output nodes

        // No depth surcharge: cost 5 at 10 nodes/sec = 0.5s.
        assert_eq!(
            throttle(10).calculate_delay(&payload, 0),
            Duration::from_millis(500)
        );

        // Depth surcharge included: cost 5 + 5 * 2 = 15 at 15 nodes/sec = 1s.
        assert_eq!(
            throttle(15).calculate_delay(&payload, 5),
            Duration::from_secs(1)
        );

        // A higher rate paces the same payload proportionally faster.
        assert!(
            throttle(20).calculate_delay(&payload, 0) < throttle(10).calculate_delay(&payload, 0)
        );
    }

    #[tokio::test(start_paused = true)]
    async fn wrap_delivers_two_payloads_per_second() {
        // A 5-node payload at 10 nodes/sec is held 5 / 10 = 0.5s. The first is delivered immediately,
        // then each subsequent one is paced 0.5s behind, so delivery holds at two per second.
        let payload = value!({ "a": 1, "b": 2, "c": 3, "d": 4 }); // 5 output nodes
        let responses = vec![
            Response::new(payload.clone()),
            Response::new(payload.clone()),
            Response::new(payload),
        ];
        let mut paced =
            Box::pin(throttle(10).wrap(futures::stream::iter(responses), QueryDepth::default()));

        let start = tokio::time::Instant::now();
        paced.next().await.unwrap();
        assert_eq!(start.elapsed(), Duration::ZERO);
        paced.next().await.unwrap();
        assert_eq!(start.elapsed(), Duration::from_millis(500));
        paced.next().await.unwrap();
        assert_eq!(start.elapsed(), Duration::from_secs(1));
        assert!(paced.next().await.is_none());
    }

    /// Exercise the production throttle boundary with payloads and query depth generated by
    /// async-graphql and the same query-limits extension used by subscription requests.
    #[tokio::test(start_paused = true)]
    async fn wrap_paces_graphql_payloads_by_size_and_validated_depth() {
        let schema = test_schema();

        let lean_depth = QueryDepth::default();
        let mut lean = Box::pin(throttle(20).wrap(
            schema.execute_stream(
                Request::new("subscription { payloads { leaf } }").data(lean_depth.clone()),
            ),
            lean_depth,
        ));
        let start = tokio::time::Instant::now();
        assert_eq!(
            lean.next().await.unwrap().data,
            value!({ "payloads": { "leaf": 1 } })
        );
        lean.next().await.unwrap();
        // Three output nodes plus depth two cost seven at twenty nodes per second.
        assert_eq!(start.elapsed(), Duration::from_millis(350));

        let rich_depth = QueryDepth::default();
        let mut rich = Box::pin(
            throttle(20).wrap(
                schema.execute_stream(
                    Request::new("subscription { payloads { leaf nested { left middle right } } }")
                        .data(rich_depth.clone()),
                ),
                rich_depth,
            ),
        );
        let start = tokio::time::Instant::now();
        assert_eq!(
            rich.next().await.unwrap().data,
            value!({
                "payloads": {
                    "leaf": 1,
                    "nested": { "left": 2, "middle": 3, "right": 4 }
                }
            })
        );
        rich.next().await.unwrap();
        // Seven output nodes plus depth three cost thirteen at twenty nodes per second.
        assert_eq!(start.elapsed(), Duration::from_millis(650));
    }

    /// A zero budget must bypass pacing for real GraphQL subscription responses.
    #[tokio::test(start_paused = true)]
    async fn wrap_does_not_pace_graphql_subscription_when_disabled() {
        let schema = test_schema();
        let query_depth = QueryDepth::default();
        let mut paced = Box::pin(
            throttle(0).wrap(
                schema.execute_stream(
                    Request::new("subscription { payloads { leaf nested { left middle right } } }")
                        .data(query_depth.clone()),
                ),
                query_depth,
            ),
        );

        let start = tokio::time::Instant::now();
        paced.next().await.unwrap();
        paced.next().await.unwrap();
        assert_eq!(start.elapsed(), Duration::ZERO);
    }

    #[tokio::test(start_paused = true)]
    async fn wrap_observes_one_delay_sample_per_payload() {
        let metric = test_histogram();
        let payload = value!({ "a": 1, "b": 2 });
        let responses = vec![Response::new(payload.clone()), Response::new(payload)];
        let mut paced = Box::pin(
            Throttle::new(10, metric.clone())
                .wrap(futures::stream::iter(responses), QueryDepth::default()),
        );

        while paced.next().await.is_some() {}

        assert_eq!(metric.get_sample_count(), 2);
    }
}
