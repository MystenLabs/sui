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
    register_histogram_with_registry, register_int_counter_with_registry, Histogram, IntCounter,
    Registry,
};
use sui_types::{
    base_types::{AuthorityName, ExecutionDigests},
    error::{SuiError, SuiResult},
    messages::{CertifiedTransaction, TransactionInfoRequest},
    messages_checkpoint::{
        AuthenticatedCheckpoint, AuthorityCheckpointInfo, CertifiedCheckpointSummary,
        CheckpointContents, CheckpointDigest, CheckpointFragment, CheckpointRequest,
        CheckpointResponse, CheckpointSequenceNumber, SignedCheckpointSummary,
    },
};
use tokio::time::Instant;

use crate::{
    authority::AuthorityState,
    authority_aggregator::{AuthorityAggregator, ReduceOutput},
    authority_client::AuthorityAPI,
    checkpoints::{proposal::CheckpointProposal, CheckpointStore},
    node_sync::NodeSyncState,
};
use sui_storage::node_sync_store::NodeSyncStore;
use sui_types::committee::{Committee, StakeUnit};
use tracing::{debug, info, warn};

#[cfg(test)]
pub(crate) mod tests;

use super::ActiveAuthority;

#[derive(Clone)]
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
}

impl Default for CheckpointProcessControl {
    /// Standard parameters (currently set heuristically).
    fn default() -> CheckpointProcessControl {
        CheckpointProcessControl {
            delay_on_quorum_failure: Duration::from_secs(10),
            delay_on_local_failure: Duration::from_secs(3),
            long_pause_between_checkpoints: Duration::from_secs(60),
            timeout_until_quorum: Duration::from_secs(60),
            extra_time_after_quorum: Duration::from_millis(200),
            consensus_delay_estimate: Duration::from_secs(1),
            per_other_authority_delay: Duration::from_secs(30),
        }
    }
}

#[derive(Clone)]
pub struct CheckpointMetrics {
    checkpoint_certificates_stored: IntCounter,
    checkpoints_signed: IntCounter,
    checkpoint_frequency: Histogram,
    checkpoint_num_fragments_sent: Histogram,
}

impl CheckpointMetrics {
    pub fn new(registry: &Registry) -> Self {
        Self {
            checkpoint_certificates_stored: register_int_counter_with_registry!(
                "checkpoint_certificates_stored",
                "Total number of unique checkpoint certificates stored in this validator",
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
            checkpoint_num_fragments_sent: register_histogram_with_registry!(
                "checkpoint_num_fragments_sent",
                "Number of fragments sent to consensus before this validator is able to make a checkpoint",
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

    let mut last_cert_time = Instant::now();

    loop {
        let net = active_authority.net.load().deref().clone();
        let committee = &net.committee;
        if committee != active_authority.state.committee.load().deref().deref() {
            warn!("Inconsistent committee between authority state and authority active");
            tokio::time::sleep(Duration::from_millis(100)).await;
            continue;
        }
        // (1) Get the latest summaries and proposals
        // TODO: This may not work if we are many epochs behind: we won't be able to download
        // from the current network. We will need to consolidate sync implementation.
        let state_of_world = get_latest_proposal_and_checkpoint_from_all(
            net.clone(),
            timing.extra_time_after_quorum,
            timing.timeout_until_quorum,
        )
        .await;

        let (checkpoint, proposals) = match state_of_world {
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
        if let Some(checkpoint) = checkpoint {
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

            let result =
                update_latest_checkpoint(active_authority, &state_checkpoints, &checkpoint).await;
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
                    let _name = state_checkpoints.lock().name;
                    let _next_checkpoint = state_checkpoints.lock().next_checkpoint();
                    metrics.checkpoint_certificates_stored.inc();
                    metrics
                        .checkpoint_frequency
                        .observe(last_cert_time.elapsed().as_secs_f64());
                    last_cert_time = Instant::now();
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
        // Only move to propose when we have the full checkpoint certificate
        let sequence_number = state_checkpoints.lock().next_checkpoint();
        if sequence_number > 0 {
            // Check that we have the full certificate for the previous checkpoint.
            // If not, we are not ready yet to make a proposal.
            if !matches!(
                state_checkpoints.lock().get_checkpoint(sequence_number - 1),
                Ok(Some(AuthenticatedCheckpoint::Certified(..)))
            ) {
                tokio::time::sleep(timing.delay_on_local_failure).await;
                continue;
            }
        }

        let proposal = state_checkpoints.lock().set_proposal(committee.epoch);

        // (4) Check if we need to advance to the next checkpoint, in case >2/3
        // have a proposal out. If so we start creating and injecting fragments
        // into the consensus protocol to make the new checkpoint.
        let weight: StakeUnit = proposals
            .iter()
            .map(|(auth, _)| committee.weight(auth))
            .sum();

        if weight < committee.quorum_threshold() {
            // We don't have a quorum of proposals yet, we won't succeed making a checkpoint
            // even if we try. Sleep and come back latter.
            tokio::time::sleep(timing.delay_on_local_failure).await;
            continue;
        }

        // (5) Now we try to create fragments and construct checkpoint.
        match proposal {
            Ok(my_proposal) => {
                if create_fragments_and_make_checkpoint(
                    active_authority,
                    state_checkpoints.clone(),
                    &my_proposal,
                    // We use the list of validators that responded with a proposal
                    // to download proposal details.
                    proposals.into_iter().map(|(name, _)| name).collect(),
                    committee,
                    timing.consensus_delay_estimate,
                    &metrics,
                )
                .await
                {
                    info!(cp_seq=?my_proposal.sequence_number(), "A new checkpoint is created and signed locally");
                    metrics.checkpoints_signed.inc();
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

/// The latest certified checkpoint can either be a checkpoint downloaded from another validator,
/// or constructed locally using a quorum of signed checkpoints. In the latter case, we won't be
/// able to download it from anywhere, but only need contents to make sure we can update it.
/// Such content can either be obtained locally if there was already a signed checkpoint, or
/// downloaded from other validators if not available.
async fn update_latest_checkpoint<A>(
    active_authority: &ActiveAuthority<A>,
    state_checkpoints: &Arc<Mutex<CheckpointStore>>,
    checkpoint: &CertifiedCheckpointSummary,
) -> SuiResult<bool>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let latest_local_checkpoint = state_checkpoints.lock().latest_stored_checkpoint()?;
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
            state_checkpoints
                .lock()
                .process_new_checkpoint_certificate(
                    checkpoint,
                    &contents,
                    committee,
                    active_authority.state.database.clone(),
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
) -> SuiResult
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let net = active_authority.net.load();
    let state = active_authority.state.clone();
    // Get out last checkpoint
    let latest_checkpoint = checkpoint_db.lock().latest_stored_checkpoint()?;
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

        sync_checkpoint_certs(
            state.clone(),
            active_authority.node_sync_store.clone(),
            net.clone(),
            &contents,
        )
        .await?;

        checkpoint_db.lock().process_new_checkpoint_certificate(
            &past,
            &contents,
            &net.committee,
            active_authority.state.database.clone(),
        )?;
    }

    Ok(())
}

/// Fetch and execute all certificates in the checkpoint.
async fn sync_checkpoint_certs<A>(
    state: Arc<AuthorityState>,
    node_sync_store: Arc<NodeSyncStore>,
    net: Arc<AuthorityAggregator<A>>,
    contents: &CheckpointContents,
) -> SuiResult
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    let sync = NodeSyncState::new(state, net, node_sync_store);
    sync.sync_checkpoint(contents).await
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
        &CheckpointRequest::past(sequence_number, contents),
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
    mut available_authorities: BTreeSet<AuthorityName>,
    committee: &Committee,
    consensus_delay_estimate: Duration,
    metrics: &CheckpointMetrics,
) -> bool
where
    A: AuthorityAPI + Send + Sync + 'static + Clone,
{
    // Pick another authority, get their proposal, and submit it to consensus
    // Exit when we have a checkpoint proposal.

    available_authorities.remove(&active_authority.state.name); // remove ourselves

    let next_checkpoint_sequence_number = checkpoint_db.lock().next_checkpoint();
    let mut fragments_num = 0;

    for authority in committee.shuffle_by_stake() {
        // We have ran out of authorities?
        if available_authorities.is_empty() {
            // We have created as many fragments as possible, so exit.
            break;
        }
        if !available_authorities.remove(authority) {
            continue;
        }

        // Get a client
        let client = active_authority.net.load().authority_clients[authority].clone();

        if let Ok(response) = client
            .handle_checkpoint(CheckpointRequest::latest(true))
            .await
        {
            if let AuthorityCheckpointInfo::Proposal { current, previous } = &response.info {
                // Check if there is a latest checkpoint
                if let AuthenticatedCheckpoint::Certified(prev) = previous {
                    if prev.summary.sequence_number >= next_checkpoint_sequence_number {
                        // We are now behind, stop the process
                        break;
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
                    // Target validator could be byzantine, just ignore it.
                    continue;
                }

                let other_proposal = CheckpointProposal::new(
                    current.as_ref().unwrap().clone(),
                    response.detail.unwrap(),
                );

                let fragment = my_proposal.fragment_with(&other_proposal);

                // We need to augment the fragment with the missing transactions
                match augment_fragment_with_diff_transactions(active_authority, fragment).await {
                    Ok(fragment) => {
                        // On success send the fragment to consensus
                        let proposer = fragment.proposer.authority();
                        let other = fragment.other.authority();
                        debug!("Send fragment: {proposer:?} -- {other:?}");
                        let _ = checkpoint_db.lock().submit_local_fragment_to_consensus(
                            &fragment,
                            &active_authority.state.committee.load(),
                        );
                    }
                    Err(err) => {
                        // TODO: some error occurred -- log it.
                        warn!("Error augmenting the fragment: {err:?}");
                    }
                }

                fragments_num += 1;

                let result = checkpoint_db.lock().attempt_to_construct_checkpoint(
                    active_authority.state.database.clone(),
                    committee,
                );

                match result {
                    Err(err) => {
                        // We likely don't have enough fragments, keep trying.
                        debug!(
                            ?next_checkpoint_sequence_number,
                            ?fragments_num,
                            "Failed to construct checkpoint: {:?}",
                            err
                        );
                        // TODO: here we should really wait until the fragment is sequenced, otherwise
                        //       we would be going ahead and sequencing more fragments that may not be
                        //       needed. For the moment we just linearly back-off.
                        tokio::time::sleep(fragments_num * consensus_delay_estimate).await;
                        continue;
                    }
                    Ok(()) => {
                        // A new checkpoint has been made.
                        metrics
                            .checkpoint_num_fragments_sent
                            .observe(fragments_num as f64);
                        return true;
                    }
                }
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
