// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use parking_lot::Mutex;
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests, TransactionDigest},
    error::SuiError,
    messages::{CertifiedTransaction, ConfirmationTransaction, TransactionInfoRequest},
    messages_checkpoint::{
        AuthenticatedCheckpoint, AuthorityCheckpointInfo, CertifiedCheckpointSummary,
        CheckpointContents, CheckpointDigest, CheckpointFragment, CheckpointRequest,
        CheckpointResponse, CheckpointSequenceNumber, SignedCheckpointSummary,
    },
};
use tokio::time::timeout;

use crate::{
    authority_aggregator::{AuthorityAggregator, ReduceOutput},
    authority_client::AuthorityAPI,
    checkpoints::{proposal::CheckpointProposal, CheckpointStore},
};
use sui_types::committee::StakeUnit;
use tracing::{debug, info, warn};
// use typed_store::Map;

#[cfg(test)]
pub(crate) mod tests;

use super::ActiveAuthority;

pub struct CheckpointProcessControl {
    /// The time to allow upon quorum failure for sufficient
    /// authorities to come online, to proceed with the checkpointing
    /// main loop.
    pub delay_on_quorum_failure: Duration,

    /// The time between full iterations of the checkpointing
    /// logic loop.
    pub long_pause_between_checkpoints: Duration,

    /// The time we allow until a quorum of responses
    /// is received.
    pub timeout_until_quorum: Duration,

    /// The time we allow after a quorum is received for
    /// additional responses to arrive.
    pub extra_time_after_quorum: Duration,

    /// The estimate of the consensus delay.
    pub consensus_delay_estimate: Duration,

    /// The amount of time we wait on any specific authority
    /// per request (it could be byzantine)
    pub per_other_authority_delay: Duration,
}

impl Default for CheckpointProcessControl {
    /// Standard parameters (currently set heuristically).
    fn default() -> CheckpointProcessControl {
        CheckpointProcessControl {
            delay_on_quorum_failure: Duration::from_secs(10),
            long_pause_between_checkpoints: Duration::from_secs(60),
            timeout_until_quorum: Duration::from_secs(60),
            extra_time_after_quorum: Duration::from_millis(200),
            consensus_delay_estimate: Duration::from_secs(3),
            per_other_authority_delay: Duration::from_secs(30),
        }
    }
}

pub async fn checkpoint_process<A>(
    active_authority: &ActiveAuthority<A>,
    timing: &CheckpointProcessControl,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    if active_authority.state.checkpoints.is_none() {
        // If the checkpointing database is not present, do not
        // operate the active checkpointing logic.
        return;
    }
    info!("Start active checkpoint process.");

    // Safe to unwrap due to check above
    let state_checkpoints = active_authority.state.checkpoints.as_ref().unwrap().clone();

    tokio::time::sleep(timing.long_pause_between_checkpoints).await;

    loop {
        let net = active_authority.net.load().deref().clone();
        let committee = &net.committee;
        if committee != active_authority.state.committee.load().deref().deref() {
            warn!("Inconsistent committee between authority state and authority active");
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }
        // (1) Get the latest summaries and proposals
        let state_of_world = get_latest_proposal_and_checkpoint_from_all(
            net.clone(),
            timing.extra_time_after_quorum,
            timing.timeout_until_quorum,
        )
        .await;

        if let Err(err) = state_of_world {
            warn!("Cannot get a quorum of checkpoint information: {:?}", err);
            // Sleep for 10 sec to allow the network to set itself up or the partition
            // to go away.
            tokio::time::sleep(timing.delay_on_quorum_failure).await;
            continue;
        }

        let (checkpoint, proposals) = state_of_world.expect("Just checked that we are not Err");

        // (2) Sync to the latest checkpoint, this might take some time.
        // Its ok nothing else goes on in terms of the active checkpoint logic
        // while we do sync. We are in any case not in a position to make valuable
        // proposals.
        if let Some(checkpoint) = checkpoint {
            // Check if there are more historic checkpoints to catch up with
            let next_checkpoint = state_checkpoints.lock().next_checkpoint();
            if next_checkpoint < checkpoint.summary.sequence_number {
                // TODO log error
                if let Err(err) = sync_to_checkpoint(
                    active_authority.state.name,
                    net.clone(),
                    state_checkpoints.clone(),
                    checkpoint.clone(),
                )
                .await
                {
                    warn!("Failure to sync to checkpoint: {}", err);
                    // if there was an error we pause to wait for network to come up
                    tokio::time::sleep(timing.delay_on_quorum_failure).await;
                }

                continue;
            }

            // The checkpoint we received is equal to what is expected or greater.
            // In either case try to upgrade the signed checkpoint to a certified one
            // if possible
            let result = {
                state_checkpoints.lock().handle_checkpoint_certificate(
                    &checkpoint,
                    &None,
                    committee,
                )
            }; // unlock

            if let Err(err) = result {
                warn!("Cannot process checkpoint: {err:?}");
                drop(err);

                // One of the errors may be due to the fact that we do not have
                // the full contents of the checkpoint. So we try to download it.
                // TODO: clean up the errors to get here only when the error is
                //       "No checkpoint set at this sequence."
                if let Ok(contents) =
                    get_checkpoint_contents(active_authority.state.name, net.clone(), &checkpoint)
                        .await
                {
                    // Retry with contents
                    let _ = state_checkpoints.lock().handle_checkpoint_certificate(
                        &checkpoint,
                        &Some(contents),
                        committee,
                    );
                }
            }
        }

        // (3) Process any unprocessed transactions. We do this before trying to move to the
        //     next proposal.
        // if let Err(err) = process_unprocessed_digests(
        //     active_authority,
        //     state_checkpoints.clone(),
        //     timing.per_other_authority_delay,
        // )
        // .await
        // {
        //     warn!("Error processing unprocessed: {:?}", err);
        //     // Nothing happens until we catch up with the unprocessed transactions of the
        //     // previous checkpoint.
        //     continue;
        // }

        // (4) Check if we need to advance to the next checkpoint, in case >2/3
        // have a proposal out. If so we start creating and injecting fragments
        // into the consensus protocol to make the new checkpoint.
        let weight: StakeUnit = proposals
            .iter()
            .map(|(auth, _)| committee.weight(auth))
            .sum();

        let _start_checkpoint_making = weight > committee.quorum_threshold();

        let proposal = state_checkpoints
            .lock()
            .new_proposal(committee.epoch)
            .clone();
        if let Ok(my_proposal) = proposal {
            diff_proposals(
                active_authority,
                state_checkpoints.clone(),
                &my_proposal,
                proposals,
                timing.consensus_delay_estimate,
            )
            .await;
        }

        // (5) Wait for a long long time.
        let name = state_checkpoints.lock().name;
        let next_checkpoint = state_checkpoints.lock().next_checkpoint();

        debug!("{name:?} at checkpoint {next_checkpoint:?}");
        tokio::time::sleep(timing.long_pause_between_checkpoints).await;
    }
}

/// Reads the latest checkpoint / proposal info from all validators
/// and extracts the latest checkpoint as well as the set of proposals
pub async fn get_latest_proposal_and_checkpoint_from_all<A>(
    net: Arc<AuthorityAggregator<A>>,
    timeout_after_quorum: Duration,
    timeout_until_quorum: Duration,
) -> Result<
    (
        Option<CertifiedCheckpointSummary>,
        Vec<(AuthorityName, SignedCheckpointSummary)>,
    ),
    SuiError,
>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    #[derive(Default)]
    struct CheckpointSummaries {
        good_weight: StakeUnit,
        bad_weight: StakeUnit,
        responses: Vec<(
            AuthorityName,
            Option<SignedCheckpointSummary>,
            AuthenticatedCheckpoint,
        )>,
        errors: Vec<(AuthorityName, SuiError)>,
    }
    let initial_state = CheckpointSummaries::default();
    let threshold = net.committee.quorum_threshold();
    let validity = net.committee.validity_threshold();
    let final_state = net
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

                        // Add to the list of errors.
                        match result {
                            Err(err) => state.errors.push((name, err)),
                            Ok(msg) => state.errors.push((
                                name,
                                SuiError::GenericAuthorityError {
                                    error: format!("Unexpected message: {:?}", msg),
                                },
                            )),
                        }

                        // Return all errors if a quorum is not possible.
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
            timeout_until_quorum,
        )
        .await?;

    // Extract the highest checkpoint cert returned.
    let mut highest_certificate_cert: Option<CertifiedCheckpointSummary> = None;
    for state in &final_state.responses {
        if let AuthenticatedCheckpoint::Certified(cert) = &state.2 {
            if let Some(old_cert) = &highest_certificate_cert {
                if cert.summary.sequence_number > old_cert.summary.sequence_number {
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
        (CheckpointSequenceNumber, CheckpointDigest),
        Vec<(AuthorityName, SignedCheckpointSummary)>,
    > = BTreeMap::new();
    final_state
        .responses
        .iter()
        .for_each(|(auth, _proposal, checkpoint)| {
            if let AuthenticatedCheckpoint::Signed(signed) = checkpoint {
                // We check this signature is higher than the highest known checkpoint.
                if let Some(newest_checkpoint) = &highest_certificate_cert {
                    if newest_checkpoint.summary.sequence_number > signed.summary.sequence_number {
                        return;
                    }
                }

                // Collect signed checkpoints by sequence number and digest.
                partial_checkpoints
                    .entry((signed.summary.sequence_number, signed.summary.digest()))
                    .or_insert_with(Vec::new)
                    .push((*auth, signed.clone()));
            }
        });

    // We use a BTreeMap here to ensure we iterate in increasing order of checkpoint
    // sequence numbers. If we find a valid checkpoint we are sure this is the highest.
    partial_checkpoints
        .iter()
        .for_each(|((_seq, _digest), signed)| {
            let weight: StakeUnit = signed
                .iter()
                .map(|(auth, _)| net.committee.weight(auth))
                .sum();

            // Reminder: a valid checkpoint only contains a validity threshold (1/3 N + 1) of signatures.
            //           The reason is that if >3/2 of node fragments are used to construct the checkpoint
            //           only 1/3N + 1 honest nodes are guaranteed to be able to fully reconstruct and sign
            //           the checkpoint for others to download.
            if weight >= net.committee.validity_threshold() {
                // Try to construct a valid checkpoint.
                let certificate = CertifiedCheckpointSummary::aggregate(
                    signed.iter().map(|(_, signed)| signed.clone()).collect(),
                    &net.committee,
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
        .map(|cert| cert.summary.sequence_number + 1)
        .unwrap_or(0);

    // Collect proposals
    let proposals: Vec<_> = final_state
        .responses
        .iter()
        .filter_map(|(auth, proposal, _checkpoint)| {
            if let Some(p) = proposal {
                if p.summary.sequence_number == next_proposal_sequence_number {
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
    name: AuthorityName,
    net: Arc<AuthorityAggregator<A>>,
    checkpoint_db: Arc<Mutex<CheckpointStore>>,
    latest_known_checkpoint: CertifiedCheckpointSummary,
) -> Result<(), SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Get out last checkpoint
    let latest_checkpoint = checkpoint_db.lock().latest_stored_checkpoint()?;
    // We use the latest available authorities not the authorities that signed the checkpoint
    // since these might be gone after the epoch they were active.
    let available_authorities: BTreeSet<_> = latest_known_checkpoint
        .signatory_authorities()
        .cloned()
        .collect();

    // Check if the latest checkpoint is merely a signed checkpoint, and if
    // so download a full certificate for it.
    if let Some(AuthenticatedCheckpoint::Signed(signed)) = &latest_checkpoint {
        let seq = *signed.summary.sequence_number();
        debug!("Partial Sync ({name:?}): {seq:?}",);
        let (past, _contents) =
            get_one_checkpoint(net.clone(), seq, false, &available_authorities).await?;

        if let Err(err) =
            checkpoint_db
                .lock()
                .handle_checkpoint_certificate(&past, &None, &net.committee)
        {
            warn!("Error handling certificate: {err:?}");
        }
    }

    let full_sync_start = latest_checkpoint
        .map(|chk| match chk {
            AuthenticatedCheckpoint::Signed(signed) => signed.summary.sequence_number + 1,
            AuthenticatedCheckpoint::Certified(cert) => cert.summary.sequence_number + 1,
            AuthenticatedCheckpoint::None => unreachable!(),
        })
        .unwrap_or(0);

    for seq in full_sync_start..latest_known_checkpoint.summary.sequence_number {
        debug!("Full Sync ({name:?}): {seq:?}");
        let (past, _contents) =
            get_one_checkpoint(net.clone(), seq, true, &available_authorities).await?;

        if let Err(err) =
            checkpoint_db
                .lock()
                .handle_checkpoint_certificate(&past, &_contents, &net.committee)
        {
            warn!("Sync Err: {err:?}");
        }
    }

    Ok(())
}

/// Gets one checkpoint certificate and optionally its contents. Note this must be
/// given a checkpoint number that the validator knows exists, for examples because
/// they have seen a subsequent certificate.
#[allow(clippy::collapsible_match)]
pub async fn get_one_checkpoint<A>(
    net: Arc<AuthorityAggregator<A>>,
    sequence_number: CheckpointSequenceNumber,
    contents: bool,
    available_authorities: &BTreeSet<AuthorityName>,
) -> Result<(CertifiedCheckpointSummary, Option<CheckpointContents>), SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let mut available_authorities = available_authorities.clone();
    while !available_authorities.is_empty() {
        // Get a random authority by stake
        let sample_authority = net.committee.sample();
        if !available_authorities.contains(sample_authority) {
            // We want to pick an authority that has the checkpoint and its full history.
            continue;
        }

        available_authorities.remove(sample_authority);

        // Note: safe to do lookup since authority comes from the committee sample
        //       so this should not panic.
        let client = net.clone_client(sample_authority);
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
                warn!("Sync Error: Unexpected answer: {resp:?}");
            }
            Err(err) => {
                warn!("Sync Error: peer error: {err:?}");
            }
        }
    }

    Err(SuiError::GenericAuthorityError {
        error: "Used all authorities but did not get a valid previous checkpoint.".to_string(),
    })
}

/// Given a checkpoint certificate we sample validators and try to download the certificate contents.
#[allow(clippy::collapsible_match)]
pub async fn get_checkpoint_contents<A>(
    name: AuthorityName,
    net: Arc<AuthorityAggregator<A>>,
    checkpoint: &CertifiedCheckpointSummary,
) -> Result<CheckpointContents, SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let mut available_authorities: BTreeSet<_> =
        checkpoint.signatory_authorities().cloned().collect();
    available_authorities.remove(&name);

    loop {
        // Get a random authority by stake
        let sample_authority = net.committee.sample();
        if !available_authorities.contains(sample_authority) {
            // We want to pick an authority that has the checkpoint and its full history.
            continue;
        }

        // Note: safe to do lookup since authority comes from the committee sample
        //       so this should not panic.
        let client = net.clone_client(sample_authority);
        match client
            .handle_checkpoint(CheckpointRequest::past(
                checkpoint.summary.sequence_number,
                true,
            ))
            .await
        {
            Ok(CheckpointResponse {
                info: _info,
                detail: Some(contents),
            }) => {
                // Check here that the digest of contents matches
                if contents.digest() != checkpoint.summary.content_digest {
                    // A byzantine authority!
                    // TODO: Report Byzantine authority
                    warn!("Sync Error: Incorrect contents returned");
                    continue;
                }

                return Ok(contents);
            }
            Ok(resp) => {
                warn!("Sync Error: Unexpected answer: {resp:?}");
            }
            Err(err) => {
                warn!("Sync Error: peer error: {err:?}");
            }
        }
    }
}

/// Picks other authorities at random and constructs checkpoint fragments
/// that are submitted to consensus. The process terminates when a future
/// checkpoint is downloaded
pub async fn diff_proposals<A>(
    active_authority: &ActiveAuthority<A>,
    checkpoint_db: Arc<Mutex<CheckpointStore>>,
    my_proposal: &CheckpointProposal,
    proposals: Vec<(AuthorityName, SignedCheckpointSummary)>,
    consensus_delay_estimate: Duration,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Pick another authority, get their proposal, and submit it to consensus
    // Exit when we have a checkpoint proposal.

    let mut available_authorities: BTreeSet<_> = proposals.iter().map(|(auth, _)| *auth).collect();
    available_authorities.remove(&active_authority.state.name); // remove ourselves
    let mut fragments_num = 0;

    loop {
        let next_checkpoint_sequence_number: u64 = checkpoint_db.lock().next_checkpoint();
        if next_checkpoint_sequence_number > *my_proposal.signed_summary.summary.sequence_number() {
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

        let random_authority = *active_authority.net.load().committee.sample();
        if available_authorities.remove(&random_authority) {
            // Get a client
            let client = active_authority.net.load().authority_clients[&random_authority].clone();

            if let Ok(response) = client
                .handle_checkpoint(CheckpointRequest::latest(true))
                .await
            {
                if let AuthorityCheckpointInfo::Proposal { current, previous } = &response.info {
                    // Check if there is a latest checkpoint
                    if let AuthenticatedCheckpoint::Certified(prev) = previous {
                        if prev.summary.sequence_number > next_checkpoint_sequence_number {
                            // We are now way behind, return
                            return;
                        }
                    }

                    // For some reason the proposal is empty?
                    if current.is_none() || response.detail.is_none() {
                        continue;
                    }

                    // Check the proposal is also for the same checkpoint sequence number
                    if current.as_ref().unwrap().summary.sequence_number()
                        != my_proposal.sequence_number()
                    {
                        return;
                    }

                    let other_proposal = CheckpointProposal::new(
                        current.as_ref().unwrap().clone(),
                        response.detail.unwrap(),
                    );

                    let fragment = my_proposal.fragment_with(&other_proposal);

                    // We need to augment the fragment with the missing transactions
                    match augment_fragment_with_diff_transactions(active_authority, fragment).await
                    {
                        Ok(fragment) => {
                            // On success send the fragment to consensus
                            let proposer = fragment.proposer.authority();
                            let other = fragment.other.authority();
                            debug!("Send fragment: {proposer:?} -- {other:?}");
                            let _ = checkpoint_db.lock().handle_receive_fragment(
                                &fragment,
                                &active_authority.state.committee.load(),
                            );
                        }
                        Err(err) => {
                            // TODO: some error occurred -- log it.
                            warn!("Error augmenting the fragment: {err:?}");
                        }
                    }

                    // TODO: here we should really wait until the fragment is sequenced, otherwise
                    //       we would be going ahead and sequencing more fragments that may not be
                    //       needed. For the moment we just linearly back-off.
                    fragments_num += 1;
                    if fragments_num > 2 {
                        tokio::time::sleep(fragments_num * consensus_delay_estimate).await;
                    }
                }
            } else {
                continue;
            }
        }
    }
}

/// Given a fragment with this authority as the proposer and another authority as the counterpart,
/// augment the fragment with all actual certificates corresponding to the differences. Some will
/// come from the local database, but others will come from downloading them from the other
/// authority.
pub async fn augment_fragment_with_diff_transactions<A>(
    active_authority: &ActiveAuthority<A>,
    mut fragment: CheckpointFragment,
) -> Result<CheckpointFragment, SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let mut diff_certs: BTreeMap<ExecutionDigests, CertifiedTransaction> = BTreeMap::new();

    // These are the trasnactions that we have that the other validator does not
    // have, so we can read them from our local database.
    for tx_digest in &fragment.diff.second.items {
        let cert = active_authority
            .state
            .read_certificate(&tx_digest.transaction)
            .await?
            .ok_or(SuiError::CertificateNotfound {
                certificate_digest: tx_digest.transaction,
            })?;
        diff_certs.insert(*tx_digest, cert);
    }

    // These are the transactions that the other node has, so we have to potentially
    // download them from the remote node.
    let client = active_authority
        .net
        .load()
        .clone_client(fragment.other.authority());
    for tx_digest in &fragment.diff.first.items {
        let response = client
            .handle_transaction_info_request(TransactionInfoRequest::from(tx_digest.transaction))
            .await?;
        let cert = response
            .certified_transaction
            .ok_or(SuiError::CertificateNotfound {
                certificate_digest: tx_digest.transaction,
            })?;
        diff_certs.insert(*tx_digest, cert);
    }

    if !diff_certs.is_empty() {
        let len = diff_certs.len();
        debug!("Augment fragment with: {len:?} tx");
    }

    // Augment the fragment in place.
    fragment.certs = diff_certs;

    Ok(fragment)
}

/// Looks into the unprocessed_digests and tries to process them all to allow
/// for the creation of the next proposal. Also uses the unprocessed_content
/// to look for transactions before going to fetch them from the network.
// pub async fn process_unprocessed_digests<A>(
//     active_authority: &ActiveAuthority<A>,
//     checkpoint_db: Arc<Mutex<CheckpointStore>>,
//     per_other_authority_delay: Duration,
// ) -> Result<(), SuiError>
// where
//     A: AuthorityAPI + Send + Sync + 'static + Clone,
// {
//     let unprocessed_digests: Vec<_> = checkpoint_db
//         .lock()
//         .unprocessed_transactions
//         .iter()
//         .map(|(digest, _)| digest)
//         .collect();

//     let existing_certificates = checkpoint_db
//         .lock()
//         .unprocessed_contents
//         .multi_get(&unprocessed_digests)?;

//     // First process all certs that we have stored in the unprocessed_contents
//     let mut processed = BTreeSet::new();
//     for (digest, cert) in unprocessed_digests
//         .iter()
//         .zip(existing_certificates.iter())
//         .filter_map(|(digest, cert_opt)| cert_opt.as_ref().map(|c| (digest, c)))
//     {
//         active_authority
//             .net
//             .load()
//             .sync_certificate_to_authority_with_timeout(
//                 ConfirmationTransaction::new(cert.clone()),
//                 active_authority.state.name,
//                 per_other_authority_delay,
//                 3,
//             )
//             .await?;
//         processed.insert(digest);
//     }

//     for digest in &unprocessed_digests {
//         // If we have processed this continue with the next cert, nothing to do
//         if active_authority
//             .state
//             .database
//             .effects_exists(&digest.transaction)?
//         {
//             continue;
//         }

//         // Download the certificate
//         debug!("Try sync for digest: {digest:?}");
//         if let Err(err) = sync_digest(
//             active_authority.state.name,
//             active_authority.net.load().clone(),
//             digest.transaction,
//             per_other_authority_delay,
//         )
//         .await
//         {
//             warn!("Error doing sync from digest {digest:?}: {err}");
//             return Err(err);
//         }
//     }

//     Ok(())
// }

/// Sync to a transaction certificate
pub async fn sync_digest<A>(
    name: AuthorityName,
    net: Arc<AuthorityAggregator<A>>,
    cert_digest: TransactionDigest,
    timeout_period: Duration,
) -> Result<(), SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let mut source_authorities: BTreeSet<AuthorityName> =
        net.committee.voting_rights.keys().copied().collect();
    source_authorities.remove(&name);

    // Now try to update the destination authority sequentially using
    // the source authorities we have sampled.
    debug_assert!(!source_authorities.is_empty());
    for source_authority in source_authorities {
        // Note: here we could improve this function by passing into the
        //       `sync_authority_source_to_destination` call a cache of
        //       certificates and parents to avoid re-downloading them.

        let client = net.clone_client(&source_authority);
        let sync_fut = async {
            let response = client
                .handle_transaction_info_request(TransactionInfoRequest::from(cert_digest))
                .await?;

            // If we have cert, use that cert to sync
            if let Some(cert) = response.certified_transaction {
                net.sync_certificate_to_authority_with_timeout(
                    ConfirmationTransaction::new(cert.clone()),
                    name,
                    // Ok to have a fixed, and rather long timeout, since the future is controlled,
                    // and interrupted by a global timeout as well, that can be controlled.
                    Duration::from_secs(60),
                    3,
                )
                .await?;

                return Result::<(), SuiError>::Ok(());
            }

            // If we have a transaction, make a cert and sync
            if let Some(transaction) = response.signed_transaction {
                // Make a cert afresh
                let (cert, _effects) = net
                    .execute_transaction(&transaction.to_transaction())
                    .await
                    .map_err(|_e| SuiError::AuthorityUpdateFailure)?;

                // Make sure the cert is syned with this authority
                net.sync_certificate_to_authority_with_timeout(
                    ConfirmationTransaction::new(cert.clone()),
                    name,
                    // Ok to have a fixed, and rather long timeout, since the future is controlled,
                    // and interrupted by a global timeout as well, that can be controlled.
                    Duration::from_secs(60),
                    3,
                )
                .await?;

                return Result::<(), SuiError>::Ok(());
            }

            Err(SuiError::AuthorityUpdateFailure)
        };

        // Be careful.  timeout() returning OK just means the Future completed.
        if let Ok(inner_res) = timeout(timeout_period, sync_fut).await {
            match inner_res {
                Ok(_) => {
                    // If the updates succeeds we return, since there is no need
                    // to try other sources.
                    return Ok(());
                }
                // Getting here means the sync_authority_source fn finished within timeout but errored out.
                Err(_err) => {
                    warn!("Failed sync with {:?}", source_authority);
                }
            }
        } else {
            warn!("Timeout exceeded");
        }

        // If we are here it means that the update failed, either due to the
        // source being faulty or the destination being faulty.
        //
        // TODO: We should probably be keeping a record of suspected faults
        // upon failure to de-prioritize authorities that we have observed being
        // less reliable.
    }

    // Eventually we should add more information to this error about the destination
    // and maybe event the certificate.
    Err(SuiError::AuthorityUpdateFailure)
}
