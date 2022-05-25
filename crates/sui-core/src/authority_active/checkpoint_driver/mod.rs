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
        AuthenticatedCheckpoint, AuthorityCheckpointInfo, CertifiedCheckpoint, CheckpointContents,
        CheckpointRequest, CheckpointResponse, CheckpointSequenceNumber, SignedCheckpoint,
        SignedCheckpointProposal,
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
    if _active_authority.state.checkpoints.is_none() {
        // If the checkpointing database is not present, do not
        // operate the active checkpointing logic.
        return;
    }

    // Safe to unwrap due to check above
    let state_checkpoints = _active_authority
        .state
        .checkpoints
        .as_ref()
        .unwrap()
        .clone();

    tokio::time::sleep(Duration::from_millis(1220)).await;

    loop {
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
            let next_checkpoint = state_checkpoints.lock().next_checkpoint();
            if next_checkpoint < checkpoint.checkpoint.sequence_number {
                // TODO log error
                let _ = sync_to_checkpoint(
                    _active_authority,
                    state_checkpoints.clone(),
                    checkpoint.clone(),
                )
                .await;
                // And start from the beginning, when done
                tokio::time::sleep(Duration::from_millis(100)).await;
                continue;
            }

            // Check if the checkpoint is the one we are expecting next!
            // if next_checkpoint == checkpoint.checkpoint.sequence_number {
            // Try to upgrade the signed checkpoint to a certified one if possible
            if state_checkpoints
                .lock()
                .handle_checkpoint_certificate(&checkpoint, &None)
                .is_err()
            {
                // One of the errors may be due to the fact that we do not have
                // the full contents of the checkpoint. So we try to download it.
                // TODO: clean up the errors to get here only when the error is
                //       "No checkpoint set at this sequence."
                if let Ok(contents) = get_checkpoint_contents(_active_authority, &checkpoint).await
                {
                    // Retry with contents
                    let _ = state_checkpoints
                        .lock()
                        .handle_checkpoint_certificate(&checkpoint, &Some(contents));
                }
            }
            // }
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

        // Wait for a long long time.
        let name = state_checkpoints.lock().name;
        let next_checkpoint = state_checkpoints.lock().next_checkpoint();

        println!("{:?} at checkpoint {:?}", name, next_checkpoint);
        tokio::time::sleep(Duration::from_millis(1220)).await;
    }
}

/// Reads the latest checkpoint / proposal info from all validators
/// and extracts the latest checkpoint as well as the set of proposals
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

    Ok((highest_certificate_cert, proposals))
}

/// Download all checkpoints that are not known to us
pub async fn sync_to_checkpoint<A>(
    _active_authority: &ActiveAuthority<A>,
    checkpoint_db: Arc<Mutex<CheckpointStore>>,
    latest_known_checkpoint: CertifiedCheckpoint,
) -> Result<(), SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Get out last checkpoint
    let latest_checkpoint = checkpoint_db.lock().latest_stored_checkpoint()?;
    let available_authorities: BTreeSet<_> = latest_known_checkpoint
        .signatory_authorities()
        .cloned()
        .collect();

    // Check if the latest checkpoint is merely a signed checkpoint, and if
    // so download a full certificate for it.
    if let Some(AuthenticatedCheckpoint::Signed(signed)) = &latest_checkpoint {
        println!(
            "Partial Sync ({:?}): {:?}",
            _active_authority.state.name,
            *signed.checkpoint.sequence_number()
        );
        let (past, _contents) = get_one_checkpoint(
            _active_authority,
            *signed.checkpoint.sequence_number(),
            false,
            &available_authorities,
        )
        .await?;

        // NOTE: should we ignore the error here?
        let _ = checkpoint_db
            .lock()
            .handle_checkpoint_certificate(&past, &None);
    }

    let full_sync_start = latest_checkpoint
        .map(|chk| match chk {
            AuthenticatedCheckpoint::Signed(signed) => signed.checkpoint.sequence_number + 1,
            AuthenticatedCheckpoint::Certified(cert) => cert.checkpoint.sequence_number + 1,
            AuthenticatedCheckpoint::None => unreachable!(),
        })
        .unwrap_or(0);

    for seq in full_sync_start..latest_known_checkpoint.checkpoint.sequence_number {
        println!("Full Sync ({:?}): {:?}", _active_authority.state.name, seq);
        let (past, _contents) =
            get_one_checkpoint(_active_authority, seq, true, &available_authorities).await?;
        // NOTE: should we ignore the error here?
        if let Err(err) = checkpoint_db
            .lock()
            .handle_checkpoint_certificate(&past, &_contents)
        {
            println!("Sync Err: {:?}", err);
        }
    }

    Ok(())
}

/// Gets one checkpoint certificate and optionally its contents. Note this must be
/// given a checkpoint number that the validator knows exists, for examples because
/// they have seen a subsequent certificate.
#[allow(clippy::collapsible_match)]
pub async fn get_one_checkpoint<A>(
    _active_authority: &ActiveAuthority<A>,
    sequence_number: CheckpointSequenceNumber,
    contents: bool,
    available_authorities: &BTreeSet<AuthorityName>,
) -> Result<(CertifiedCheckpoint, Option<CheckpointContents>), SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let mut available_authorities = available_authorities.clone();
    while !available_authorities.is_empty() {
        // Get a random authority by stake
        let sample_authority = _active_authority.state.committee.sample();
        if !available_authorities.contains(sample_authority) {
            // We want to pick an authority that has the checkpoint and its full history.
            continue;
        }

        available_authorities.remove(sample_authority);

        // Note: safe to do lookup since authority comes from the committee sample
        //       so this should not panic.
        let client = _active_authority.net.authority_clients[sample_authority].clone();
        match client
            .handle_checkpoint(CheckpointRequest::past(sequence_number, contents))
            .await
        {
            Ok(CheckpointResponse {
                info: AuthorityCheckpointInfo::Past(AuthenticatedCheckpoint::Certified(past)),
                detail,
            }) => {
                return Ok((past, detail));
            }
            Ok(resp) => {
                println!("Sync Error: Unexpected answer: {:?}", resp);
            }
            Err(err) => {
                println!("Sync Error: peer error: {:?}", err);
            }
        }
    }

    Err(SuiError::GenericAuthorityError {
        error: "Ran out of authorities.".to_string(),
    })
}

/// Given a checkpoint certificate we sample validators and try to download the certificate contents.
#[allow(clippy::collapsible_match)]
pub async fn get_checkpoint_contents<A>(
    _active_authority: &ActiveAuthority<A>,
    checkpoint: &CertifiedCheckpoint,
) -> Result<CheckpointContents, SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let available_authorities: BTreeSet<_> = checkpoint.signatory_authorities().cloned().collect();
    loop {
        // Get a random authority by stake
        let sample_authority = _active_authority.state.committee.sample();
        if !available_authorities.contains(sample_authority) {
            // We want to pick an authority that has the checkpoint and its full history.
            continue;
        }

        // Note: safe to do lookup since authority comes from the committee sample
        //       so this should not panic.
        let client = _active_authority.net.authority_clients[sample_authority].clone();
        match client
            .handle_checkpoint(CheckpointRequest::past(
                checkpoint.checkpoint.sequence_number,
                true,
            ))
            .await
        {
            Ok(CheckpointResponse {
                info: _info,
                detail: Some(contents),
            }) => {
                // TODO: check here that the digest of contents matches
                if contents.digest() != checkpoint.checkpoint.digest {
                    // A byzantine authority!
                    println!("Sync Error: Incorrect contents returned");
                    continue;
                }

                return Ok(contents);
            }
            Ok(resp) => {
                println!("Sync Error: Unexpected answer: {:?}", resp);
            }
            Err(err) => {
                println!("Sync Error: peer error: {:?}", err);
            }
        }
    }
}

/// Picks other authorities at random and constructs checkpoint fragments
/// that are submitted to consensus. The process terminates when a future
/// checkpoint is downloaded
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
    let mut fragments_num = 0;

    loop {
        let next_checkpoint_sequence_number: u64 = checkpoint_db.lock().next_checkpoint();
        if next_checkpoint_sequence_number > *_my_proposal.proposal.0.checkpoint.sequence_number() {
            // Our work here is done, we have progressed past the checkpoint for which we were given a proposal.
            // Our DB has been updated (presumably by consensus) with the sought information (a checkpoint
            // for this sequence number)
            break;
        }

        // We have ran out of authorities?
        if available_authorities.is_empty() {
            // We have created as many fragments as possible, so exit.
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

                    // For some reason the proposal is empty?
                    if current.is_none() || response.detail.is_none() {
                        continue;
                    }

                    // TODO: check the proposal is also for the same checkpoint sequence number?
                    if current.as_ref().unwrap().0.checkpoint.sequence_number()
                        != _my_proposal.sequence_number()
                    {
                        return;
                    }

                    let other_proposal = CheckpointProposal::new(
                        current.as_ref().unwrap().clone(),
                        response.detail.unwrap(),
                    );

                    let fragment = _my_proposal.fragment_with(&other_proposal);
                    let _ = checkpoint_db.lock().handle_receive_fragment(&fragment);

                    // TODO: here we should really wait until the fragment is sequenced, otherwise
                    //       we would be going ahead and sequencing more fragments that may not be
                    //       needed. For the moment we just linearly back-off.
                    fragments_num += 1;
                    if fragments_num > 2 {
                        tokio::time::sleep(Duration::from_secs(3 * fragments_num)).await;
                    }
                }
            } else {
                continue;
            }
        } else {
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }
}
