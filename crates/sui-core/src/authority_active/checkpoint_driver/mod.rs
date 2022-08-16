// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use parking_lot::Mutex;
use prometheus::{
    register_histogram_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntGauge, Registry,
};
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests},
    error::{SuiError, SuiResult},
    messages::{CertifiedTransaction, TransactionInfoRequest},
    messages_checkpoint::{
        AuthenticatedCheckpoint, AuthorityCheckpointInfo, CertifiedCheckpointSummary,
        CheckpointContents, CheckpointDigest, CheckpointFragment, CheckpointProposal,
        CheckpointRequest, CheckpointResponse, CheckpointSequenceNumber, SignedCheckpointSummary,
    },
};
use tokio::time::Instant;

use futures::stream::StreamExt;

use crate::{
    authority_aggregator::{AuthorityAggregator, ReduceOutput},
    authority_client::AuthorityAPI,
    checkpoints::CheckpointStore,
    epoch::reconfiguration::Reconfigurable,
};

use sui_types::committee::{Committee, StakeUnit};
use tracing::{debug, error, info, warn};

#[cfg(test)]
pub(crate) mod tests;

use super::ActiveAuthority;

#[derive(Clone, Debug)]
pub struct CheckpointProcessControl {
    /// The time to allow upon quorum failure for sufficient
    /// authorities to come online, to proceed with the checkpointing
    /// main loop.
    pub delay_on_quorum_failure: Duration,

    /// The delay before we retry the process, when there is a local error
    /// that prevented us from making progress, e.g. failed to create
    /// a new proposal, or not ready to set a new checkpoint due to unexecuted transactions.
    pub delay_on_local_failure: Duration,

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

    /// The amount if time we wait before retrying anything
    /// during an epoch change. We want this duration to be very small
    /// to minimize the amount of time to finish epoch change.
    pub epoch_change_retry_delay: Duration,
}

impl Default for CheckpointProcessControl {
    /// Standard parameters (currently set heuristically).
    fn default() -> CheckpointProcessControl {
        CheckpointProcessControl {
            delay_on_quorum_failure: Duration::from_secs(10),
            delay_on_local_failure: Duration::from_secs(3),
            long_pause_between_checkpoints: Duration::from_secs(120),
            timeout_until_quorum: Duration::from_secs(60),
            extra_time_after_quorum: Duration::from_millis(200),
            // TODO: Optimize this.
            // https://github.com/MystenLabs/sui/issues/3619.
            consensus_delay_estimate: Duration::from_secs(2),
            per_other_authority_delay: Duration::from_secs(30),
            epoch_change_retry_delay: Duration::from_millis(100),
        }
    }
}

#[derive(Clone)]
pub struct CheckpointMetrics {
    pub checkpoint_sequence_number: IntGauge,
    checkpoints_signed: IntCounter,
    checkpoint_frequency: Histogram,
}

impl CheckpointMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            checkpoint_sequence_number: register_int_gauge_with_registry!(
                "checkpoint_sequence_number",
                "Latest sequence number of certified checkpoint stored in this validator",
                registry,
            )
            .unwrap(),
            checkpoints_signed: register_int_counter_with_registry!(
                "checkpoints_signed",
                "Total number of checkpoints signed by this validator",
                registry,
            )
            .unwrap(),
            checkpoint_frequency: register_histogram_with_registry!(
                "checkpoint_frequency",
                "Number of seconds elapsed between two consecutive checkpoint certificates",
                registry,
            )
            .unwrap(),
        }
    }

    pub fn new_for_tests() -> Self {
        let registry = Registry::new();
        Self::new(&registry)
    }
}

pub async fn checkpoint_process<A>(
    active_authority: &ActiveAuthority<A>,
    timing: &CheckpointProcessControl,
    metrics: CheckpointMetrics,
    enable_reconfig: bool,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone + Reconfigurable,
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

    let mut last_cert_time = Instant::now();

    loop {
        let net = active_authority.net.load().deref().clone();
        let committee = &net.committee;
        if committee != active_authority.state.committee.load().deref().deref() {
            warn!("Inconsistent committee between authority state and authority active");
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }
        // (1) Get the latest checkpoint cert from the network.
        // TODO: This may not work if we are many epochs behind: we won't be able to download
        // from the current network. We will need to consolidate sync implementation.
        let highest_checkpoint = get_latest_checkpoint_from_all(
            net.clone(),
            timing.extra_time_after_quorum,
            timing.timeout_until_quorum,
        )
        .await;

        let highest_checkpoint = match highest_checkpoint {
            Ok(s) => s,
            Err(err) => {
                warn!("Cannot get a quorum of checkpoint information: {:?}", err);
                // Sleep for delay_on_quorum_failure to allow the network to set itself
                // up or the partition to go away.
                tokio::time::sleep(timing.delay_on_quorum_failure).await;
                continue;
            }
        };

        // (2) Sync to the latest checkpoint, this might take some time.
        // Its ok nothing else goes on in terms of the active checkpoint logic
        // while we do sync. We are in any case not in a position to make valuable
        // proposals.
        if let Some(checkpoint) = highest_checkpoint {
            debug!(
                "Highest Checkpoint Certificate from the network: {}",
                checkpoint
            );
            // Check if there are more historic checkpoints to catch up with
            let next_checkpoint = state_checkpoints.lock().next_checkpoint();
            // First sync until before the latest checkpoint. We will special
            // handle the latest checkpoint latter.
            if next_checkpoint < checkpoint.summary.sequence_number {
                info!(
                    cp_seq=?next_checkpoint,
                    latest_cp_seq=?checkpoint.summary.sequence_number,
                    "Checkpoint is behind the latest in the network, start syncing",
                );
                // TODO: The sync process only works within an epoch.
                if let Err(err) = sync_to_checkpoint(
                    active_authority,
                    state_checkpoints.clone(),
                    checkpoint.clone(),
                    &metrics,
                )
                .await
                {
                    warn!("Failure to sync to checkpoint: {:?}", err);
                    // if there was an error we pause to wait for network to come up
                    tokio::time::sleep(timing.delay_on_quorum_failure).await;
                }
                last_cert_time = Instant::now();
                debug!("Checkpoint sync finished");
                // The above process can take some time, and the latest checkpoint may have
                // already changed. Restart process to be sure.
                continue;
            }

            let result = update_latest_checkpoint(
                active_authority,
                &state_checkpoints,
                &checkpoint,
                &metrics,
            )
            .await;
            match result {
                Err(err) => {
                    warn!(
                        "{:?} failed to update latest checkpoint: {:?}",
                        active_authority.state.name, err
                    );
                    tokio::time::sleep(timing.delay_on_local_failure).await;
                    continue;
                }
                Ok(true) => {
                    metrics
                        .checkpoint_frequency
                        .observe(last_cert_time.elapsed().as_secs_f64());
                    last_cert_time = Instant::now();
                    if enable_reconfig {
                        if state_checkpoints.lock().is_ready_to_start_epoch_change() {
                            while let Err(err) = active_authority.start_epoch_change().await {
                                error!("Failed to start epoch change: {:?}", err);
                                tokio::time::sleep(timing.epoch_change_retry_delay).await;
                            }
                            // No long pause to minimize the reconfiguration latency.
                            continue;
                        } else if state_checkpoints.lock().is_ready_to_finish_epoch_change() {
                            while let Err(err) = active_authority.finish_epoch_change().await {
                                error!("Failed to finish epoch change: {:?}", err);
                                tokio::time::sleep(timing.epoch_change_retry_delay).await;
                            }
                        }
                    }
                    tokio::time::sleep(timing.long_pause_between_checkpoints).await;
                    continue;
                }
                Ok(false) => {
                    // Nothing new.
                    // Falling through to start checkpoint making process.
                }
            }
        }

        // (3) Create a new proposal locally. This will also allow other validators
        // to query the proposal.
        // If we have already signed a new checkpoint locally, there is nothing to do.
        if matches!(
            state_checkpoints.lock().latest_stored_checkpoint(),
            Some(AuthenticatedCheckpoint::Signed(..))
        ) {
            tokio::time::sleep(timing.delay_on_local_failure).await;
            continue;
        }

        let proposal = state_checkpoints.lock().set_proposal(committee.epoch);

        // (5) Now we try to create fragments and construct checkpoint.
        // TODO: Restructure the fragment making process.
        match proposal {
            Ok(my_proposal) => {
                if create_fragments_and_make_checkpoint(
                    active_authority,
                    state_checkpoints.clone(),
                    &my_proposal,
                    committee,
                    timing.consensus_delay_estimate,
                )
                .await
                {
                    info!(cp_seq=?my_proposal.sequence_number(), "A new checkpoint is created and signed locally");
                    metrics.checkpoints_signed.inc();
                } else {
                    debug!("Failed to make checkpoint after going through all available proposals");
                    tokio::time::sleep(timing.delay_on_local_failure).await;
                }
            }
            Err(err) => {
                warn!(
                    "{:?} failed to make a new proposal: {:?}",
                    active_authority.state.name, err
                );
                tokio::time::sleep(timing.delay_on_local_failure).await;
                continue;
            }
        }
    }
}

/// Obtain the highest checkpoint certificate from all validators.
/// It's done by querying the latest authenticated checkpoints from a quorum of validators.
/// If we get a quorum of signed checkpoints of the same sequence number, form a cert on the fly.
pub async fn get_latest_checkpoint_from_all<A>(
    net: Arc<AuthorityAggregator<A>>,
    timeout_after_quorum: Duration,
    timeout_until_quorum: Duration,
) -> Result<Option<CertifiedCheckpointSummary>, SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    #[derive(Default)]
    struct CheckpointSummaries {
        good_weight: StakeUnit,
        bad_weight: StakeUnit,
        responses: Vec<(AuthorityName, Option<AuthenticatedCheckpoint>)>,
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
                        .handle_checkpoint(CheckpointRequest::authenticated(None, false))
                        .await
                })
            },
            |mut state, name, weight, result| {
                Box::pin(async move {
                    if let Ok(CheckpointResponse {
                        info: AuthorityCheckpointInfo::AuthenticatedCheckpoint(checkpoint),
                        ..
                    }) = result
                    {
                        state.responses.push((name, checkpoint));
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
                                action: "get_latest_checkpoint_from_all",
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
        if let Some(AuthenticatedCheckpoint::Certified(cert)) = &state.1 {
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
    final_state.responses.iter().for_each(|(auth, checkpoint)| {
        if let Some(AuthenticatedCheckpoint::Signed(signed)) = checkpoint {
            // We are interested in this signed checkpoint only if it is
            // newer than the highest known cert checkpoint.
            if let Some(newest_checkpoint) = &highest_certificate_cert {
                if newest_checkpoint.summary.sequence_number >= signed.summary.sequence_number {
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
        .for_each(|((seq, _digest), signed)| {
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
                    debug!(cp_seq=?seq, "A checkpoint certificate is formed from the network");
                    highest_certificate_cert = Some(cert);
                }
            }
        });

    Ok(highest_certificate_cert)
}

/// The latest certified checkpoint can either be a checkpoint downloaded from another validator,
/// or constructed locally using a quorum of signed checkpoints. In the latter case, we won't be
/// able to download it from anywhere, but only need contents to make sure we can update it.
/// Such content can either be obtained locally if there was already a signed checkpoint, or
/// downloaded from other validators if not available.
async fn update_latest_checkpoint<A>(
    active_authority: &ActiveAuthority<A>,
    state_checkpoints: &Arc<Mutex<CheckpointStore>>,
    checkpoint: &CertifiedCheckpointSummary,
    metrics: &CheckpointMetrics,
) -> SuiResult<bool>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let latest_local_checkpoint = state_checkpoints.lock().latest_stored_checkpoint();
    enum Action {
        Promote,
        NewCert,
        Nothing,
    }
    let seq = checkpoint.summary.sequence_number;
    let action = match latest_local_checkpoint {
        None => Action::NewCert,
        Some(AuthenticatedCheckpoint::Certified(c)) if c.summary.sequence_number + 1 == seq => {
            Action::NewCert
        }
        Some(AuthenticatedCheckpoint::Signed(s)) if s.summary.sequence_number == seq => {
            Action::Promote
        }
        Some(a) => {
            assert!(
                a.summary().sequence_number >= seq,
                "We should have run sync before this"
            );
            Action::Nothing
        }
    };
    let self_name = active_authority.state.name;
    let committee = &active_authority.net.load().committee;
    match action {
        Action::Promote => {
            state_checkpoints
                .lock()
                .promote_signed_checkpoint_to_cert(checkpoint, committee, metrics)?;
            info!(
                cp_seq=?checkpoint.summary.sequence_number(),
                "Updated local signed checkpoint to certificate",
            );
            Ok(true)
        }
        Action::NewCert => {
            let available_authorities: BTreeSet<_> = checkpoint
                .signatory_authorities(committee)
                .filter_map(|x| match x {
                    Ok(&a) => {
                        if a != self_name {
                            Some(Ok(a))
                        } else {
                            None
                        }
                    }
                    Err(e) => Some(Err(e)),
                })
                .collect::<SuiResult<_>>()?;
            let (_, contents) = get_one_checkpoint_with_contents(
                active_authority.net.load().clone(),
                checkpoint.summary.sequence_number,
                &available_authorities,
            )
            .await?;
            state_checkpoints
                .lock()
                .process_new_checkpoint_certificate(
                    checkpoint,
                    &contents,
                    committee,
                    active_authority.state.database.clone(),
                    metrics,
                )?;
            info!(
                cp_seq=?checkpoint.summary.sequence_number(),
                "Stored new checkpoint certificate",
            );
            Ok(true)
        }
        Action::Nothing => Ok(false),
    }
}

/// Download all checkpoints that are not known to us
pub async fn sync_to_checkpoint<A>(
    active_authority: &ActiveAuthority<A>,
    checkpoint_db: Arc<Mutex<CheckpointStore>>,
    latest_known_checkpoint: CertifiedCheckpointSummary,
    metrics: &CheckpointMetrics,
) -> SuiResult
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let net = active_authority.net.load();
    let state = active_authority.state.clone();
    // Get out last checkpoint
    let latest_checkpoint = checkpoint_db.lock().latest_stored_checkpoint();
    // We use the latest available authorities not the authorities that signed the checkpoint
    // since these might be gone after the epoch they were active.
    let available_authorities: BTreeSet<_> = latest_known_checkpoint
        .signatory_authorities(&net.committee)
        .collect::<SuiResult<BTreeSet<_>>>()?
        .iter()
        .map(|&&x| x)
        .collect();

    // Check if the latest checkpoint is merely a signed checkpoint, and if
    // so download a full certificate for it.
    if let Some(AuthenticatedCheckpoint::Signed(signed)) = &latest_checkpoint {
        let seq = *signed.summary.sequence_number();
        debug!(name = ?state.name, ?seq, "Partial Sync",);
        let (past, _) = get_one_checkpoint(net.clone(), seq, false, &available_authorities).await?;

        checkpoint_db
            .lock()
            .promote_signed_checkpoint_to_cert(&past, &net.committee, metrics)?;
    }

    let full_sync_start = latest_checkpoint
        .map(|chk| chk.summary().sequence_number + 1)
        .unwrap_or(0);

    for seq in full_sync_start..latest_known_checkpoint.summary.sequence_number {
        debug!(name = ?state.name, ?seq, "Full Sync",);
        let (past, contents) =
            get_one_checkpoint_with_contents(net.clone(), seq, &available_authorities).await?;

        let errors = active_authority
            .node_sync_handle()
            .sync_checkpoint(&contents)
            .await?
            .zip(futures::stream::iter(contents.iter()))
            .filter_map(|(r, digests)| async move {
                r.map_err(|e| {
                    info!(?digests, "failed to execute digest from checkpoint: {}", e);
                    e
                })
                .err()
            })
            .collect::<Vec<SuiError>>()
            .await;

        if !errors.is_empty() {
            let error = "Failed to sync transactions in checkpoint".to_string();
            error!(?seq, "{}", error);
            return Err(SuiError::CheckpointingError { error });
        }

        checkpoint_db.lock().process_new_checkpoint_certificate(
            &past,
            &contents,
            &net.committee,
            active_authority.state.database.clone(),
            metrics,
        )?;
    }

    Ok(())
}

pub async fn get_one_checkpoint_with_contents<A>(
    net: Arc<AuthorityAggregator<A>>,
    sequence_number: CheckpointSequenceNumber,
    available_authorities: &BTreeSet<AuthorityName>,
) -> Result<(CertifiedCheckpointSummary, CheckpointContents), SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    get_one_checkpoint(net, sequence_number, true, available_authorities)
        .await
        // unwrap ok because of true param above.
        .map(|ok| (ok.0, ok.1.unwrap()))
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
    net.get_certified_checkpoint(
        sequence_number,
        contents,
        available_authorities,
        // Loop forever until we get the cert from someone.
        None,
    )
    .await
}

/// Picks other authorities at random and constructs checkpoint fragments
/// that are submitted to consensus. The process terminates when a future
/// checkpoint is created, or we run out of validators.
/// Returns whether we have successfully created and signed a new checkpoint.
pub async fn create_fragments_and_make_checkpoint<A>(
    active_authority: &ActiveAuthority<A>,
    checkpoint_db: Arc<Mutex<CheckpointStore>>,
    my_proposal: &CheckpointProposal,
    committee: &Committee,
    consensus_delay_estimate: Duration,
) -> bool
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let mut available_authorities = committee.shuffle_by_stake(None, None);
    // Remove ourselves and all validators that we have already diffed with.
    let already_fragmented = checkpoint_db.lock().validators_already_fragmented_with();
    // TODO: We can also use AuthorityHealth to pick healthy authorities first.
    available_authorities
        .retain(|name| name != &active_authority.state.name && !already_fragmented.contains(name));
    debug!(
        fragmented_count=?already_fragmented.len(),
        to_be_fragmented_count=?available_authorities.len(),
        "Going through remaining validators to generate fragments",
    );

    let next_checkpoint_sequence_number = checkpoint_db.lock().next_checkpoint();
    let mut index = 0;

    loop {
        // Always try to construct checkpoint first. This gives a chance to construct checkpoint
        // even when `available_authorities` is empty already.
        let result = checkpoint_db
            .lock()
            .attempt_to_construct_checkpoint(active_authority.state.database.clone(), committee);

        match result {
            Err(err) => {
                // We likely don't have enough fragments. Fall through to send more fragments.
                debug!(
                    ?next_checkpoint_sequence_number,
                    num_proposals_processed=?index,
                    "Failed to construct checkpoint: {:?}",
                    err
                );
            }
            Ok(()) => {
                // A new checkpoint has been made.
                return true;
            }
        }

        if checkpoint_db.lock().get_locals().no_more_fragments {
            // Sending more fragments won't help anymore.
            break;
        }

        // We have ran out of authorities?
        if index == available_authorities.len() {
            // We have created as many fragments as possible, so exit.
            break;
        }
        let authority = available_authorities[index];
        index += 1;

        // Get a client
        let client = active_authority.net.load().authority_clients[&authority].clone();

        match client
            .handle_checkpoint(CheckpointRequest::proposal(true))
            .await
        {
            Ok(response) => {
                if let AuthorityCheckpointInfo::CheckpointProposal {
                    proposal,
                    prev_cert,
                } = &response.info
                {
                    // Check if there is a latest checkpoint
                    if let Some(prev) = prev_cert {
                        if prev.summary.sequence_number >= next_checkpoint_sequence_number {
                            // We are now behind, stop the process
                            debug!(
                                latest_cp_cert_seq=?prev.summary.sequence_number,
                                expected_cp_seq=?next_checkpoint_sequence_number,
                                "We are behind, abort checkpoint construction process",
                            );
                            break;
                        }
                    }

                    // For some reason the proposal is empty?
                    if proposal.is_none() || response.detail.is_none() {
                        continue;
                    }

                    // Check the proposal is also for the same checkpoint sequence number
                    if &proposal.as_ref().unwrap().summary.sequence_number
                        != my_proposal.sequence_number()
                    {
                        // Target validator may be byzantine or behind, ignore it.
                        continue;
                    }

                    let other_proposal = CheckpointProposal::new_from_signed_proposal_summary(
                        proposal.as_ref().unwrap().clone(),
                        response.detail.unwrap(),
                    );

                    let fragment = my_proposal.fragment_with(&other_proposal);

                    // We need to augment the fragment with the missing transactions
                    match augment_fragment_with_diff_transactions(active_authority, fragment).await
                    {
                        Ok(fragment) => {
                            // On success send the fragment to consensus
                            if let Err(err) =
                                checkpoint_db.lock().submit_local_fragment_to_consensus(
                                    &fragment,
                                    &active_authority.state.committee.load(),
                                )
                            {
                                warn!("Error submitting local fragment to consensus: {err:?}");
                            }
                            // TODO: here we should really wait until the fragment is sequenced, otherwise
                            //       we would be going ahead and sequencing more fragments that may not be
                            //       needed. For the moment we just rely on linearly back-off.
                            // https://github.com/MystenLabs/sui/issues/3619.
                            tokio::time::sleep(consensus_delay_estimate.mul_f64(index as f64))
                                .await;
                        }
                        Err(err) => {
                            warn!("Error augmenting the fragment: {err:?}");
                        }
                    }
                }
            }
            Err(err) => {
                warn!(
                    "Error querying checkpoint proposal from validator {}: {:?}",
                    authority, err
                );
            }
        }
    }
    false
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
