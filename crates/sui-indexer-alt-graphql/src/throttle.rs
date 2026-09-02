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
    use async_graphql::value;

    use super::*;

    fn throttle(nodes_per_second: u32) -> Throttle {
        Throttle::new(nodes_per_second, test_histogram())
    }

    fn test_histogram() -> Histogram {
        Histogram::with_opts(prometheus::HistogramOpts::new("test", "test")).unwrap()
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

    #[tokio::test(start_paused = true)]
    async fn wrap_includes_depth_surcharge() {
        // 4-node payloads at query depth 3 each cost 4 + 3 * 2 = 10, so at 20 nodes/sec the gap
        // between deliveries is 10 / 20 = 0.5s.
        let query_depth = QueryDepth::new_for_test(3);
        let payload = value!({ "a": 1, "b": 2, "c": 3 }); // 4 output nodes
        let responses = vec![Response::new(payload.clone()), Response::new(payload)];
        let mut paced = Box::pin(throttle(20).wrap(futures::stream::iter(responses), query_depth));

        let start = tokio::time::Instant::now();
        paced.next().await.unwrap();
        assert_eq!(start.elapsed(), Duration::ZERO);
        paced.next().await.unwrap();
        assert_eq!(start.elapsed(), Duration::from_millis(500));
    }

    #[tokio::test(start_paused = true)]
    async fn wrap_does_not_pace_when_disabled() {
        // A rate of 0 disables pacing, so both payloads arrive immediately with no gap.
        let payload = value!({ "a": 1, "b": 2 });
        let responses = vec![Response::new(payload.clone()), Response::new(payload)];
        let mut paced =
            Box::pin(throttle(0).wrap(futures::stream::iter(responses), QueryDepth::default()));

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
