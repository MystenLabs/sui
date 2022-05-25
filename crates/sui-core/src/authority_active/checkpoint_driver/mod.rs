// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use parking_lot::Mutex;
use sui_types::{
    base_types::AuthorityName,
    error::SuiError,
    messages_checkpoint::{
        AuthenticatedCheckpoint, AuthorityCheckpointInfo, CertifiedCheckpoint, CheckpointRequest,
        CheckpointResponse, CheckpointSequenceNumber, SignedCheckpoint, SignedCheckpointProposal,
    },
};

use crate::{
    authority_aggregator::ReduceOutput,
    authority_client::AuthorityAPI,
    checkpoints::{proposal::CheckpointProposal, CheckpointStore},
};

#[cfg(test)]
pub(crate) mod tests;

use super::ActiveAuthority;

pub async fn checkpoint_process<A>(_active_authority: &ActiveAuthority<A>)
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    println!("START ACTIVE CHECKPOINT PROCESS!");
    if _active_authority.state._checkpoints.is_none() {
        // If the checkpointing database is not present, do not
        // operate the active checkpointing logic.
        return;
    }

    // Safe to unwrap due to check above
    let state_checkpoints = _active_authority
        .state
        ._checkpoints
        .as_ref()
        .unwrap()
        .clone();

    loop {
        // Wait for a long long time.
        tokio::time::sleep(Duration::from_millis(1220)).await;

        // First, get the latest summaries and proposals
        let (checkpoint, proposals) = get_latest_proposal_and_checkpoint_from_all(
            _active_authority,
            Duration::from_millis(200),
        )
        .await
        .expect("All ok");

        // Second, sync to the latest checkpoint, this might take some time.
        // Its ok nothing else goes on in terms of the active checkpoint logic
        // while we do sync. We are in any case not in a position to make valuable
        // proposals.
        if let Some(checkpoint) = checkpoint {
            // Check if there are more historic checkpoints to catch up with
            if state_checkpoints.lock().next_checkpoint() <= checkpoint.checkpoint.sequence_number {
                sync_to_checkpoint(_active_authority, checkpoint.clone()).await;
                // And start from the beginning, when done
                continue;
            }

            // Try to updgrade the signed checkpoint to a certified one if possible
            let _ = state_checkpoints
                .lock()
                .handle_checkpoint_certificate(&checkpoint, &None);
        }

        // Check if we need to advance to the next checkpoint, in case >2/3
        // have a proposal out.
        let weight: usize = proposals
            .iter()
            .map(|(auth, _)| _active_authority.state.committee.weight(auth))
            .sum();

        let _start_checkpoint_making =
            weight > _active_authority.state.committee.quorum_threshold();

        let proposal = state_checkpoints.lock().new_proposal().clone();
        if let Ok(my_proposal) = proposal {
            diff_proposals(
                _active_authority,
                state_checkpoints.clone(),
                &my_proposal,
                proposals,
            )
            .await;
        }
    }
}

pub async fn get_latest_proposal_and_checkpoint_from_all<A>(
    _active_authority: &ActiveAuthority<A>,
    timeout_after_quorum: Duration,
) -> Result<
    (
        Option<CertifiedCheckpoint>,
        Vec<(AuthorityName, SignedCheckpointProposal)>,
    ),
    SuiError,
>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    #[derive(Default)]
    struct CheckpointSummaries {
        good_weight: usize,
        bad_weight: usize,
        responses: Vec<(
            AuthorityName,
            Option<SignedCheckpointProposal>,
            AuthenticatedCheckpoint,
        )>,
        errors: Vec<(AuthorityName, SuiError)>,
    }
    let initial_state = CheckpointSummaries::default();
    let threshold = _active_authority.state.committee.quorum_threshold();
    let validity = _active_authority.state.committee.validity_threshold();
    let final_state = _active_authority
        .net
        .quorum_map_then_reduce_with_timeout(
            initial_state,
            |_name, client| {
                Box::pin(async move {
                    // Request and return an error if any
                    client
                        .handle_checkpoint(CheckpointRequest::latest(false))
                        .await
                })
            },
            |mut state, name, weight, result| {
                Box::pin(async move {
                    if let Ok(CheckpointResponse {
                        info: AuthorityCheckpointInfo::Proposal { current, previous },
                        ..
                    }) = result
                    {
                        state.responses.push((name, current, previous));
                        state.good_weight += weight;
                    } else {
                        state.bad_weight += weight;
                        if state.bad_weight > validity {
                            return Err(SuiError::TooManyIncorrectAuthorities {
                                errors: state.errors,
                            });
                        }
                    }

                    if state.good_weight < threshold {
                        // While we are under the threshold we wait for a longer time
                        Ok(ReduceOutput::Continue(state))
                    } else {
                        // After we reach threshold we wait for potentially less time.
                        Ok(ReduceOutput::ContinueWithTimeout(
                            state,
                            timeout_after_quorum,
                        ))
                    }
                })
            },
            // A long timeout before we hear back from a quorum
            Duration::from_secs(60),
        )
        .await?;

    // Extract the highest checkpoint cert returned.
    let mut highest_certificate_cert: Option<CertifiedCheckpoint> = None;
    for state in &final_state.responses {
        if let AuthenticatedCheckpoint::Certified(cert) = &state.2 {
            if let Some(old_cert) = &highest_certificate_cert {
                if cert.checkpoint.sequence_number > old_cert.checkpoint.sequence_number {
                    highest_certificate_cert = Some(cert.clone());
                }
            } else {
                highest_certificate_cert = Some(cert.clone());
            }
        }
    }

    // Attempt to construct a newer checkpoint from signed summaries.
    #[allow(clippy::type_complexity)]
    let mut partial_checkpoints: BTreeMap<
        (CheckpointSequenceNumber, [u8; 32]),
        Vec<(AuthorityName, SignedCheckpoint)>,
    > = BTreeMap::new();
    final_state
        .responses
        .iter()
        .for_each(|(auth, _proposal, checkpoint)| {
            if let AuthenticatedCheckpoint::Signed(signed) = checkpoint {
                // We check this signature is higher than the highest known checkpoint.
                if let Some(newest_checkpoint) = &highest_certificate_cert {
                    if newest_checkpoint.checkpoint.sequence_number
                        > signed.checkpoint.sequence_number
                    {
                        return;
                    }
                }

                // Collect signed checkpoints by sequence number and digest.
                partial_checkpoints
                    .entry((
                        signed.checkpoint.sequence_number,
                        signed.checkpoint.digest(),
                    ))
                    .or_insert_with(Vec::new)
                    .push((*auth, signed.clone()));
            }
        });

    // We use a BTreeMap here to ensure we iterate in increasing order of checkpoint
    // sequence numbers. If we find a valid checkpoint we are sure this is the higest.
    partial_checkpoints
        .iter()
        .for_each(|((_seq, _digest), signed)| {
            let weight: usize = signed
                .iter()
                .map(|(auth, _)| _active_authority.state.committee.weight(auth))
                .sum();
            if weight > _active_authority.state.committee.validity_threshold() {
                // Try to construct a valid checkpoint.
                let certificate = CertifiedCheckpoint::aggregate(
                    signed.iter().map(|(_, signed)| signed.clone()).collect(),
                    &_active_authority.state.committee,
                );
                if let Ok(cert) = certificate {
                    highest_certificate_cert = Some(cert);
                }
            }
        });

    println!(
        "HIGHEST CHK: {:?}",
        highest_certificate_cert
            .as_ref()
            .map(|x| x.checkpoint.sequence_number)
    );

    if highest_certificate_cert.is_none() {
        println!("{:?}", final_state.responses);
    }

    // Examine whether we should start the next checkpoint by looking at whether we have
    // >2/3 of validators proposing a new checkpoint.
    let next_proposal_sequence_number = highest_certificate_cert
        .as_ref()
        .map(|cert| cert.checkpoint.sequence_number + 1)
        .unwrap_or(0);

    // Collect proposals
    let proposals: Vec<_> = final_state
        .responses
        .iter()
        .filter_map(|(auth, proposal, _checkpoint)| {
            if let Some(p) = proposal {
                if p.0.checkpoint.sequence_number == next_proposal_sequence_number {
                    return Some((*auth, p.clone()));
                }
            }
            None
        })
        .collect();

    // Weight the proposals
    let weight: usize = proposals
        .iter()
        .map(|(auth, _)| _active_authority.state.committee.weight(auth))
        .sum();
    let start_checkpoint_making = weight > _active_authority.state.committee.quorum_threshold();
    println!(
        "Make checkpoint at {}: {}",
        next_proposal_sequence_number, start_checkpoint_making
    );

    Ok((highest_certificate_cert, proposals))
}

pub async fn sync_to_checkpoint<A>(
    _active_authority: &ActiveAuthority<A>,
    _checkpoint: CertifiedCheckpoint,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // TODO
}

pub async fn diff_proposals<A>(
    _active_authority: &ActiveAuthority<A>,
    checkpoint_db: Arc<Mutex<CheckpointStore>>,
    _my_proposal: &CheckpointProposal,
    _proposals: Vec<(AuthorityName, SignedCheckpointProposal)>,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Pick another authority, get their proposal, and submit it to consensus
    // Exit when we have a checkpoint proposal.

    let mut available_authorities: BTreeSet<_> = _proposals.iter().map(|(auth, _)| *auth).collect();
    available_authorities.remove(&_active_authority.state.name); // remove ourselves

    loop {
        let next_checkpoint_sequence_number: u64 = checkpoint_db.lock().next_checkpoint();
        if next_checkpoint_sequence_number > *_my_proposal.proposal.0.checkpoint.sequence_number() {
            // Our work here is done, we have progressed past the checkpoint for which we were
            // given a proposal.
            break;
        }

        // We have ran out of authorities?
        if available_authorities.is_empty() {
            break;
        }

        let random_authority = _active_authority.state.committee.sample();
        if available_authorities.remove(random_authority) {
            // Get a client
            let client = _active_authority.net.authority_clients[random_authority].clone();

            if let Ok(response) = client
                .handle_checkpoint(CheckpointRequest::latest(true))
                .await
            {
                if let AuthorityCheckpointInfo::Proposal { current, previous } = &response.info {
                    // Check if there is a latest checkpoint
                    if let AuthenticatedCheckpoint::Certified(prev) = previous {
                        if prev.checkpoint.sequence_number > next_checkpoint_sequence_number {
                            // We are now way behind, return
                            return;
                        }
                    }

                    let other_proposal = CheckpointProposal::new(
                        current.as_ref().unwrap().clone(),
                        response.detail.unwrap(),
                    );

                    // TODO: check the proposal is also for the same checkpoint sequence number?
                    let fragment = _my_proposal.fragment_with(&other_proposal);
                    let _ = checkpoint_db.lock().handle_receive_fragment(&fragment);
                }
            } else {
                continue;
            }
        }
        tokio::time::sleep(Duration::from_secs(1)).await;
    }
}
