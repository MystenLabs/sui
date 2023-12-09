// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::Future;
use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use tracing::instrument::Instrument;
use mysten_metrics::monitored_future;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::Duration;
use sui_types::committee::{CommitteeTrait, StakeUnit};
use sui_types::base_types::ConciseAbleName;

use tokio::time::timeout;

pub type AsyncResult<'a, T, E> = BoxFuture<'a, Result<T, E>>;

pub enum ReduceOutput<R, S> {
    Continue(S),
    ContinueWithTimeout(S, Duration),
    Failed(S),
    Success(R),
}

pub async fn quorum_map_then_reduce_with_timeout_and_prefs<
    'a,
    C,
    K,
    Client: 'a,
    S,
    V,
    R,
    E,
    FMap,
    FReduce,
>(
    committee: Arc<C>,
    authority_clients: Arc<BTreeMap<K, Arc<Client>>>,
    authority_preferences: Option<&BTreeSet<K>>,
    initial_state: S,
    map_each_authority: FMap,
    reduce_result: FReduce,
    initial_timeout: Duration,
) -> Result<
    (
        R,
        FuturesUnordered<impl Future<Output = (K, Result<V, E>)> + 'a>,
    ),
    S,
>
where
    K: Ord + ConciseAbleName<'a> + Copy + 'a,
    C: CommitteeTrait<K>,
    FMap: FnOnce(K, Arc<Client>) -> AsyncResult<'a, V, E> + Clone + 'a,
    FReduce: Fn(S, K, StakeUnit, Result<V, E>) -> BoxFuture<'a, ReduceOutput<R, S>>,
{
    let authorities_shuffled = committee.shuffle_by_stake(authority_preferences, None);

    // First, execute in parallel for each authority FMap.
    let mut responses: futures::stream::FuturesUnordered<_> = authorities_shuffled
        .into_iter()
        .map(|name| {
            let client = authority_clients[&name].clone();
            let execute = map_each_authority.clone();
            let concise_name = name.concise_owned();
            monitored_future!(async move {
                (
                    name,
                    execute(name, client)
                        .instrument(
                            tracing::trace_span!("quorum_map_auth", authority =? concise_name),
                        )
                        .await,
                )
            })
        })
        .collect();

    let mut current_timeout = initial_timeout;
    let mut accumulated_state = initial_state;
    // Then, as results become available fold them into the state using FReduce.
    while let Ok(Some((authority_name, result))) = timeout(current_timeout, responses.next()).await
    {
        let authority_weight = committee.weight(&authority_name);
        accumulated_state =
            match reduce_result(accumulated_state, authority_name, authority_weight, result).await {
                // In the first two cases we are told to continue the iteration.
                ReduceOutput::Continue(state) => state,
                ReduceOutput::ContinueWithTimeout(state, duration) => {
                    // Adjust the waiting timeout.
                    current_timeout = duration;
                    state
                }
                ReduceOutput::Failed(state) => {
                    return Err(state);
                }
                ReduceOutput::Success(result) => {
                    // The reducer tells us that we have the result needed. Just return it.
                    return Ok((result, responses));
                }
            }
    }
    // If we have exhausted all authorities and still have not returned a result, return
    // error with the accumulated state.
    Err(accumulated_state)
}
