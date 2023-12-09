// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! TODO: add description
use crate::bridge_client::BridgeClient;
use crate::crypto::BridgeAuthorityPublicKeyBytes;
use crate::error::{BridgeError, BridgeResult};
use crate::events::SuiBridgeEvent;
use crate::types::BridgeCommittee;
use crate::types::BridgeCommitteeValiditySignInfo;
use futures::Future;
use futures::{future::BoxFuture, stream::FuturesUnordered, StreamExt};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

pub struct BridgeQuorumDriver {
    pub committee: Arc<BridgeCommittee>,
    pub clients: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, Arc<BridgeClient>>>,
}

impl BridgeQuorumDriver {
    pub async fn get_committee_signatures(
        _event: SuiBridgeEvent,
        _committee: Arc<BridgeCommittee>,
    ) -> BridgeResult<BridgeCommitteeValiditySignInfo> {
        unimplemented!()
    }
}

// pub type AsyncResult<'a, T, E> = BoxFuture<'a, Result<T, E>>;

// pub(crate) async fn quorum_map_then_reduce_with_timeout<'a, S, V, R, FMap, FReduce>(
//     committee: Arc<BridgeCommittee>,
//     authority_clients: Arc<BTreeMap<BridgeAuthorityPublicKeyBytes, Arc<BridgeClient>>>,
//     initial_state: S,
//     map_each_authority: FMap,
//     reduce_result: FReduce,
//     initial_timeout: Duration,
// ) -> Result<
//     (
//         R,
//         FuturesUnordered<impl Future<Output = (BridgeAuthorityPublicKeyBytes, Result<V, BridgeError>)>>,
//     ),
//     S,
// >
// where
//     FMap: FnOnce(BridgeAuthorityPublicKeyBytes, Arc<BridgeClient>) -> AsyncResult<'a, V, BridgeError> + Clone,
//     FReduce: Fn(
//         S,
//         BridgeAuthorityPublicKeyBytes,
//         StakeUnit,
//         Result<V, SuiError>,
//     ) -> BoxFuture<'a, ReduceOutput<R, S>>,
// {
//     // let authorities_shuffled = committee.shuffle_by_stake(authority_preferences, None);

//     // First, execute in parallel for each authority FMap.
//     let mut responses: futures::stream::FuturesUnordered<_> = authorities_shuffled
//         .into_iter()
//         .map(|name| {
//             let client = authority_clients[&name].clone();
//             let execute = map_each_authority.clone();
//             monitored_future!(async move {
//                 (
//                     name,
//                     execute(name, client)
//                         .instrument(tracing::trace_span!("quorum_map_auth", authority =? name.concise()))
//                         .await,
//                 )
//             })
//         })
//         .collect();

//     let mut current_timeout = initial_timeout;
//     let mut accumulated_state = initial_state;
//     // Then, as results become available fold them into the state using FReduce.
//     while let Ok(Some((authority_name, result))) =
//         timeout(current_timeout, responses.next()).await
//     {
//         let authority_weight = committee.weight(&authority_name);
//         accumulated_state =
//             match reduce_result(accumulated_state, authority_name, authority_weight, result)
//                 .await
//             {
//                 // In the first two cases we are told to continue the iteration.
//                 ReduceOutput::Continue(state) => state,
//                 ReduceOutput::ContinueWithTimeout(state, duration) => {
//                     // Adjust the waiting timeout.
//                     current_timeout = duration;
//                     state
//                 }
//                 ReduceOutput::Failed(state) => {
//                     return Err(state);
//                 }
//                 ReduceOutput::Success(result) => {
//                     // The reducer tells us that we have the result needed. Just return it.
//                     return Ok((result, responses));
//                 }
//             }
//     }
//     // If we have exhausted all authorities and still have not returned a result, return
//     // error with the accumulated state.
//     Err(accumulated_state)
// }

// // Repeatedly calls the
