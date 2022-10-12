// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::ops::Deref;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};

use parking_lot::Mutex;
use prometheus::{
    linear_buckets, register_histogram_with_registry, register_int_counter_with_registry,
    register_int_gauge_with_registry, Histogram, IntCounter, IntGauge, Registry,
};
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests},
    error::{SuiError, SuiResult},
    messages::{CertifiedTransaction, TransactionInfoRequest},
    messages_checkpoint::{
        AuthenticatedCheckpoint, CertifiedCheckpointSummary, CheckpointContents, CheckpointDigest,
        CheckpointFragment, CheckpointProposal, CheckpointRequest, CheckpointResponse,
        CheckpointSequenceNumber, SignedCheckpointSummary,
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

use sui_types::committee::{Committee, EpochId, StakeUnit};
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
            consensus_delay_estimate: Duration::from_secs(3),
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
                // start from 1 min, increase by 3 min, so [1, 4, ... 58]
                // safe to unwrap because params are good
                linear_buckets(60., 180., 20).unwrap(),
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

pub enum CheckpointStepResult {
    NewCheckpointCertStored,
    CheckpointSigned,
}

pub enum CheckpointStepError {
    InconsistentCommittee,
    SyncCheckpointFromQuorumFailed(Box<SuiError>),
    UpdateLatestCheckpointFailed(Box<SuiError>),
    WaitForCheckpointCert,
    ProposalFailed(Box<SuiError>),
    CheckpointCreationFailed,
    CheckpointSignBlocked(Box<SuiError>),
}

pub async fn checkpoint_process<A>(
    active_authority: Arc<ActiveAuthority<A>>,
    timing: &CheckpointProcessControl,
    metrics: CheckpointMetrics,
) where
    A: AuthorityAPI + Send + Sync + 'static + Clone + Reconfigurable,
{
    info!("Start active checkpoint process.");

    tokio::time::sleep(timing.long_pause_between_checkpoints).await;

    let mut last_cert_time = Instant::now();

    loop {
        let result = checkpoint_process_step(active_authority.clone(), timing).await;
        let state_checkpoints = &active_authority.state.checkpoints;
        let next_cp_seq = state_checkpoints.lock().next_checkpoint();
        match result {
            Ok(result) => {
                match result {
                    CheckpointStepResult::CheckpointSigned => {
                        info!(
                            ?next_cp_seq,
                            "A new checkpoint is created and signed locally"
                        );
                        metrics.checkpoints_signed.inc();
                    }
                    CheckpointStepResult::NewCheckpointCertStored => {
                        metrics
                            .checkpoint_frequency
                            .observe(last_cert_time.elapsed().as_secs_f64());
                        metrics
                            .checkpoint_sequence_number
                            .set((next_cp_seq - 1) as i64);
                        last_cert_time = Instant::now();
                        if state_checkpoints.lock().is_ready_to_start_epoch_change() {
                            while let Err(err) = active_authority.start_epoch_change().await {
                                error!(?next_cp_seq, "Failed to start epoch change: {:?}", err);
                                tokio::time::sleep(timing.epoch_change_retry_delay).await;
                            }
                            // No delay to minimize the reconfiguration latency.
                            continue;
                        } else if state_checkpoints.lock().is_ready_to_finish_epoch_change() {
                            while let Err(err) = active_authority.finish_epoch_change().await {
                                error!(?next_cp_seq, "Failed to finish epoch change: {:?}", err);
                                tokio::time::sleep(timing.epoch_change_retry_delay).await;
                            }
                        }
                        tokio::time::sleep(timing.long_pause_between_checkpoints).await;
                    }
                }
            }
            Err(error) => {
                match error {
                    CheckpointStepError::InconsistentCommittee => {
                        warn!(
                            ?next_cp_seq,
                            "Inconsistent committee between authority state and authority active"
                        );
                    }
                    CheckpointStepError::SyncCheckpointFromQuorumFailed(err) => {
                        warn!(
                            ?next_cp_seq,
                            "Cannot get a quorum when syncing checkpoint information: {:?}", err
                        );
                        // Sleep for delay_on_quorum_failure to allow the network to set itself
                        // up or the partition to go away.
                        tokio::time::sleep(timing.delay_on_quorum_failure).await;
                    }
                    CheckpointStepError::UpdateLatestCheckpointFailed(err) => {
                        warn!(
                            ?next_cp_seq,
                            "{:?} failed to update latest checkpoint: {:?}",
                            active_authority.state.name,
                            err
                        );
                    }
                    CheckpointStepError::CheckpointCreationFailed => {
                        debug!(
                            ?next_cp_seq,
                            "Unable to make checkpoint after going through all available proposals"
                        );
                        // Extra delay to allow consensus to sequence fragments.
                        tokio::time::sleep(timing.consensus_delay_estimate).await;
                    }
                    CheckpointStepError::CheckpointSignBlocked(err) => {
                        error!(
                            ?next_cp_seq,
                            "Failed to sync and sign new checkpoint: {:?}", err
                        );
                    }
                    CheckpointStepError::ProposalFailed(err) => {
                        warn!(
                            ?next_cp_seq,
                            "{:?} failed to make a new proposal: {:?}",
                            active_authority.state.name,
                            err
                        );
                    }
                    CheckpointStepError::WaitForCheckpointCert => {
                        // This is very common due to missing transactions, nothing to do here.
                    }
                }
                tokio::time::sleep(timing.delay_on_local_failure).await;
            }
        }
    }
}

pub async fn checkpoint_process_step<A>(
    active_authority: Arc<ActiveAuthority<A>>,
    timing: &CheckpointProcessControl,
) -> Result<CheckpointStepResult, CheckpointStepError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone + Reconfigurable,
{
    let net = active_authority.net.load().deref().clone();
    let committee = &net.committee;
    if committee != active_authority.state.committee.load().deref().deref() {
        return Err(CheckpointStepError::InconsistentCommittee);
    }

    // (1) Get the latest checkpoint cert from the network.
    // TODO: This may not work if we are many epochs behind: we won't be able to download
    // from the current network. We will need to consolidate sync implementation.
    let highest_checkpoint = get_latest_checkpoint_from_all(
        net.clone(),
        timing.extra_time_after_quorum,
        timing.timeout_until_quorum,
    )
    .await
    .map_err(|err| CheckpointStepError::SyncCheckpointFromQuorumFailed(Box::new(err)))?;

    // (2) Sync to the latest checkpoint, this might take some time.
    // Its ok nothing else goes on in terms of the active checkpoint logic
    // while we do sync. We are in any case not in a position to make valuable
    // proposals.
    // Safe to unwrap due to check in the main process function.
    let state_checkpoints = &active_authority.state.checkpoints;
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
            sync_to_checkpoint(
                active_authority.clone(),
                state_checkpoints.clone(),
                checkpoint.clone(),
            )
            .await
            .map_err(|err| CheckpointStepError::SyncCheckpointFromQuorumFailed(Box::new(err)))?;
        }

        if update_latest_checkpoint(active_authority.clone(), state_checkpoints, &checkpoint)
            .await
            .map_err(|err| CheckpointStepError::UpdateLatestCheckpointFailed(Box::new(err)))?
        {
            return Ok(CheckpointStepResult::NewCheckpointCertStored);
        }
        // Nothing new.
        // Falling through to start checkpoint making process.
    }

    // If we have already signed a new checkpoint locally, there is nothing to do.
    if matches!(
        state_checkpoints.lock().latest_stored_checkpoint(),
        Some(AuthenticatedCheckpoint::Signed(..))
    ) {
        return Err(CheckpointStepError::WaitForCheckpointCert);
    }

    // (3) Create a new proposal locally. This will also allow other validators
    // to query the proposal.
    let my_proposal = state_checkpoints
        .lock()
        .set_proposal(committee.epoch)
        .map_err(|err| CheckpointStepError::ProposalFailed(Box::new(err)))?;

    // (4) Now we try to create fragments and get list of transactions for the checkpoint.
    let transactions = match create_fragments(
        active_authority.clone(),
        state_checkpoints.clone(),
        &my_proposal,
        committee,
    )
    .await
    {
        Some(contents) => contents,
        None => {
            return Err(CheckpointStepError::CheckpointCreationFailed);
        }
    };

    // (5) Execute all transactions in the checkpoint and sign it.
    sync_and_sign_new_checkpoint(
        active_authority,
        my_proposal.signed_summary.auth_signature.epoch,
        *my_proposal.sequence_number(),
        transactions,
    )
    .await
    .map_err(|err| CheckpointStepError::CheckpointSignBlocked(Box::new(err)))?;

    Ok(CheckpointStepResult::CheckpointSigned)
}

pub async fn sync_and_sign_new_checkpoint<A>(
    active_authority: Arc<ActiveAuthority<A>>,
    epoch: EpochId,
    seq: CheckpointSequenceNumber,
    transactions: BTreeSet<ExecutionDigests>,
) -> SuiResult
where
    A: AuthorityAPI + Send + Sync + 'static + Clone + Reconfigurable,
{
    let errors = active_authority
        .clone()
        .node_sync_handle()
        .sync_pending_checkpoint_transactions(epoch, transactions.iter())
        .await?
        .zip(futures::stream::iter(transactions.iter()))
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
        let error = "Failed to execute transactions in checkpoint".to_string();
        return Err(SuiError::CheckpointingError { error });
    }

    let enable_reconfig = active_authority.state.checkpoints.lock().enable_reconfig;
    let next_epoch_committee = if enable_reconfig {
        // Ready to start epoch change means that we have finalized the last second checkpoint,
        // and now we are about to finalize the last checkpoint of the epoch.
        let is_last_checkpoint = active_authority
            .state
            .checkpoints
            .lock()
            .is_ready_to_start_epoch_change();

        if is_last_checkpoint {
            // If this is the last checkpoint we are about to sign, we read the committee
            // information for the next epoch and put it into the last checkpoint.
            let sui_system_state = active_authority.state.get_sui_system_state_object().await?;
            Some(sui_system_state.get_next_epoch_committee())
        } else {
            None
        }
    } else {
        None
    };

    active_authority
        .state
        .checkpoints
        .lock()
        .sign_new_checkpoint(
            epoch,
            seq,
            transactions.iter(),
            active_authority.state.database.clone(),
            next_epoch_committee,
        )
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
                    if let Ok(CheckpointResponse::AuthenticatedCheckpoint { checkpoint, .. }) =
                        result
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
    active_authority: Arc<ActiveAuthority<A>>,
    state_checkpoints: &Arc<Mutex<CheckpointStore>>,
    checkpoint: &CertifiedCheckpointSummary,
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
                .promote_signed_checkpoint_to_cert(checkpoint, committee)?;
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
            process_new_checkpoint_certificate(
                active_authority,
                state_checkpoints,
                committee,
                checkpoint,
                &contents,
            )
            .await?;
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
    active_authority: Arc<ActiveAuthority<A>>,
    checkpoint_db: Arc<Mutex<CheckpointStore>>,
    latest_known_checkpoint: CertifiedCheckpointSummary,
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
            .promote_signed_checkpoint_to_cert(&past, &net.committee)?;
    }

    let full_sync_start = latest_checkpoint
        .map(|chk| chk.summary().sequence_number + 1)
        .unwrap_or(0);

    for seq in full_sync_start..latest_known_checkpoint.summary.sequence_number {
        debug!(name = ?state.name, ?seq, "Full Sync",);
        let (past, contents) =
            get_one_checkpoint_with_contents(net.clone(), seq, &available_authorities).await?;

        process_new_checkpoint_certificate(
            active_authority.clone(),
            &checkpoint_db,
            &net.committee,
            &past,
            &contents,
        )
        .await?;
    }

    Ok(())
}

async fn process_new_checkpoint_certificate<A>(
    active_authority: Arc<ActiveAuthority<A>>,
    checkpoint_db: &Arc<Mutex<CheckpointStore>>,
    committee: &Committee,
    checkpoint_cert: &CertifiedCheckpointSummary,
    contents: &CheckpointContents,
) -> SuiResult
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let epoch = checkpoint_cert.summary.epoch;
    let errors = active_authority
        .node_sync_handle()
        .sync_checkpoint_cert_transactions(epoch, contents)
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
        error!(cp_seq=?checkpoint_cert.summary.sequence_number, "{}", error);
        return Err(SuiError::CheckpointingError { error });
    }

    checkpoint_db
        .lock()
        .process_synced_checkpoint_certificate(checkpoint_cert, contents, committee)
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

/// Attempt to construct checkpoint content based on the fragments received so far.
/// If it didn't't succeed, pick an authority at random that we haven't seen fragments
/// with yet, make a new fragment and send to consensus.
pub async fn create_fragments<A>(
    active_authority: Arc<ActiveAuthority<A>>,
    checkpoint_db: Arc<Mutex<CheckpointStore>>,
    my_proposal: &CheckpointProposal,
    committee: &Committee,
) -> Option<BTreeSet<ExecutionDigests>>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let next_cp_seq = checkpoint_db.lock().next_checkpoint();

    let mut available_authorities = committee.shuffle_by_stake(None, None);
    // Remove ourselves and all validators that we have already diffed with.
    let already_fragmented = checkpoint_db
        .lock()
        .validators_already_fragmented_with(next_cp_seq);
    // TODO: We can also use AuthorityHealth to pick healthy authorities first.
    available_authorities
        .retain(|name| name != &active_authority.state.name && !already_fragmented.contains(name));
    debug!(
        ?next_cp_seq,
        fragmented_count=?already_fragmented.len(),
        to_be_fragmented_count=?available_authorities.len(),
        "Going through remaining validators to generate fragments",
    );

    let result = checkpoint_db.lock().attempt_to_construct_checkpoint();

    match result {
        Err(err) => {
            // We likely don't have enough fragments. Fall through to send more fragments.
            debug!(?next_cp_seq, "Failed to construct checkpoint: {:?}", err);
        }
        Ok(contents) => {
            // A new checkpoint has been made.
            return Some(contents);
        }
    }

    // If we failed to create a checkpoint, try to make more fragments.

    if checkpoint_db.lock().get_locals().no_more_fragments {
        // Sending more fragments won't help anymore.
        return None;
    }

    // We have ran out of authorities?
    if available_authorities.is_empty() {
        // We have created as many fragments as possible, so exit.
        return None;
    }
    let authority = available_authorities[0];

    // Get a client
    let client = active_authority.net.load().authority_clients[&authority].clone();

    match client
        .handle_checkpoint(CheckpointRequest::proposal(true))
        .await
    {
        Ok(response) => {
            if let CheckpointResponse::CheckpointProposal {
                proposal,
                prev_cert,
                proposal_contents,
            } = response
            {
                // Check if there is a latest checkpoint
                if let Some(prev) = prev_cert {
                    if prev.summary.sequence_number >= next_cp_seq {
                        // We are now behind, stop the process
                        debug!(
                            latest_cp_cert_seq=?prev.summary.sequence_number,
                            expected_cp_seq=?next_cp_seq,
                            "We are behind, abort checkpoint construction process",
                        );
                        return None;
                    }
                }

                // For some reason the proposal is empty?
                if proposal.is_none() || proposal_contents.is_none() {
                    return None;
                }

                // Check the proposal is also for the same checkpoint sequence number
                if &proposal.as_ref().unwrap().summary.sequence_number
                    != my_proposal.sequence_number()
                {
                    // Target validator may be byzantine or behind, ignore it.
                    return None;
                }

                let other_proposal = CheckpointProposal::new_from_signed_proposal_summary(
                    proposal.as_ref().unwrap().clone(),
                    proposal_contents.as_ref().unwrap().clone(),
                );

                let fragment = my_proposal.fragment_with(&other_proposal);

                // We need to augment the fragment with the missing transactions
                match augment_fragment_with_diff_transactions(active_authority.clone(), fragment)
                    .await
                {
                    Ok(fragment) => {
                        // On success send the fragment to consensus
                        if let Err(err) = checkpoint_db.lock().submit_local_fragment_to_consensus(
                            &fragment,
                            &active_authority.state.committee.load(),
                        ) {
                            warn!("Error submitting local fragment to consensus: {err:?}");
                        }
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
    None
}

/// Given a fragment with this authority as the proposer and another authority as the counterpart,
/// augment the fragment with all actual certificates corresponding to the differences. Some will
/// come from the local database, but others will come from downloading them from the other
/// authority.
pub async fn augment_fragment_with_diff_transactions<A>(
    active_authority: Arc<ActiveAuthority<A>>,
    mut fragment: CheckpointFragment,
) -> Result<CheckpointFragment, SuiError>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let mut diff_certs: BTreeMap<ExecutionDigests, CertifiedTransaction> = BTreeMap::new();

    // These are the transactions that we have that the other validator does not
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
