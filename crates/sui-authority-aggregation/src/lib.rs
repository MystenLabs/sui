// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use futures::Future;
use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use mysten_metrics::monitored_future;

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use sui_types::base_types::ConciseableName;
use sui_types::committee::{CommitteeTrait, StakeUnit};

use tokio::time::timeout;

pub type AsyncResult<'a, T, E> = BoxFuture<'a, Result<T, E>>;

pub struct SigRequestPrefs<K> {
    pub ordering_pref: BTreeSet<K>,
    pub prefetch_timeout: Duration,
}

pub enum ReduceOutput<R, S> {
    Continue(S),
    Failed(S),
    Success(R),
}

/// This function takes an initial state, than executes an asynchronous function (FMap) for each
/// authority, and folds the results as they become available into the state using an async function (FReduce).
///
/// prefetch_timeout: the minimum amount of time to spend trying to gather results from all authorities
/// before falling back to arrival order.
///
/// total_timeout: the maximum amount of total time to wait for results from all authorities, including
/// time spent prefetching.
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
    authority_preferences: Option<SigRequestPrefs<K>>,
    initial_state: S,
    map_each_authority: FMap,
    reduce_result: FReduce,
    total_timeout: Duration,
) -> Result<
    (
        R,
        FuturesUnordered<impl Future<Output = (K, Result<V, E>)> + 'a>,
    ),
    S,
>
where
    K: Ord + ConciseableName<'a> + Clone + 'a,
    C: CommitteeTrait<K>,
    FMap: FnOnce(K, Arc<Client>) -> AsyncResult<'a, V, E> + Clone + 'a,
    FReduce: Fn(S, K, StakeUnit, Result<V, E>) -> BoxFuture<'a, ReduceOutput<R, S>>,
{
    let (preference, prefetch_timeout) = if let Some(SigRequestPrefs {
        ordering_pref,
        prefetch_timeout,
    }) = authority_preferences
    {
        (Some(ordering_pref), Some(prefetch_timeout))
    } else {
        (None, None)
    };
    let authorities_shuffled = committee.shuffle_by_stake(preference.as_ref(), None);
    let mut accumulated_state = initial_state;
    let mut total_timeout = total_timeout;

    // First, execute in parallel for each authority FMap.
    let mut responses: futures::stream::FuturesUnordered<_> = authorities_shuffled
        .clone()
        .into_iter()
        .map(|name| {
            let client = authority_clients[&name].clone();
            let execute = map_each_authority.clone();
            monitored_future!(async move { (name.clone(), execute(name, client).await,) })
        })
        .collect();
    if let Some(prefetch_timeout) = prefetch_timeout {
        let elapsed = Instant::now();
        let prefetch_sleep = tokio::time::sleep(prefetch_timeout);
        let mut authority_to_result: BTreeMap<K, Result<V, E>> = BTreeMap::new();
        tokio::pin!(prefetch_sleep);
        // get all the sigs we can within prefetch_timeout
        loop {
            tokio::select! {
                resp = responses.next() => {
                    match resp {
                        Some((authority_name, result)) => {
                            authority_to_result.insert(authority_name, result);
                        }
                        None => {
                            // we have processed responses from the full committee so can stop early
                            break;
                        }
                    }
                }
                _ = &mut prefetch_sleep => {
                    break;
                }
            }
        }
        // process what we have up to this point
        for authority_name in authorities_shuffled {
            let authority_weight = committee.weight(&authority_name);
            if let Some(result) = authority_to_result.remove(&authority_name) {
                accumulated_state = match reduce_result(
                    accumulated_state,
                    authority_name,
                    authority_weight,
                    result,
                )
                .await
                {
                    // In the first two cases we are told to continue the iteration.
                    ReduceOutput::Continue(state) => state,
                    ReduceOutput::Failed(state) => {
                        return Err(state);
                    }
                    ReduceOutput::Success(result) => {
                        // The reducer tells us that we have the result needed. Just return it.
                        return Ok((result, responses));
                    }
                };
            }
        }
        // if we got here, fallback through the if statement to continue in arrival order on
        // the remaining validators
        total_timeout = total_timeout.saturating_sub(elapsed.elapsed());
    }

    // As results become available fold them into the state using FReduce.
    while let Ok(Some((authority_name, result))) = timeout(total_timeout, responses.next()).await {
        let authority_weight = committee.weight(&authority_name);
        accumulated_state =
            match reduce_result(accumulated_state, authority_name, authority_weight, result).await {
                // In the first two cases we are told to continue the iteration.
                ReduceOutput::Continue(state) => state,
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

/// This function takes an initial state, than executes an asynchronous function (FMap) for each
/// authority, and folds the results as they become available into the state using an async function (FReduce).
///
/// FMap can do io, and returns a result V. An error there may not be fatal, and could be consumed by the
/// MReduce function to overall recover from it. This is necessary to ensure byzantine authorities cannot
/// interrupt the logic of this function.
///
/// FReduce returns a result to a ReduceOutput. If the result is Err the function
/// shortcuts and the Err is returned. An Ok ReduceOutput result can be used to shortcut and return
/// the resulting state (ReduceOutput::End), continue the folding as new states arrive (ReduceOutput::Continue).
///
/// This function provides a flexible way to communicate with a quorum of authorities, processing and
/// processing their results into a safe overall result, and also safely allowing operations to continue
/// past the quorum to ensure all authorities are up to date (up to a timeout).
pub async fn quorum_map_then_reduce_with_timeout<
    'a,
    C,
    K,
    Client: 'a,
    S: 'a,
    V: 'a,
    R: 'a,
    E,
    FMap,
    FReduce,
>(
    committee: Arc<C>,
    authority_clients: Arc<BTreeMap<K, Arc<Client>>>,
    // The initial state that will be used to fold in values from authorities.
    initial_state: S,
    // The async function used to apply to each authority. It takes an authority name,
    // and authority client parameter and returns a Result<V>.
    map_each_authority: FMap,
    // The async function that takes an accumulated state, and a new result for V from an
    // authority and returns a result to a ReduceOutput state.
    reduce_result: FReduce,
    // The initial timeout applied to all
    initial_timeout: Duration,
) -> Result<
    (
        R,
        FuturesUnordered<impl Future<Output = (K, Result<V, E>)> + 'a>,
    ),
    S,
>
where
    K: Ord + ConciseableName<'a> + Clone + 'a,
    C: CommitteeTrait<K>,
    FMap: FnOnce(K, Arc<Client>) -> AsyncResult<'a, V, E> + Clone + 'a,
    FReduce: Fn(S, K, StakeUnit, Result<V, E>) -> BoxFuture<'a, ReduceOutput<R, S>> + 'a,
{
    quorum_map_then_reduce_with_timeout_and_prefs(
        committee,
        authority_clients,
        None,
        initial_state,
        map_each_authority,
        reduce_result,
        initial_timeout,
    )
    .await
}
