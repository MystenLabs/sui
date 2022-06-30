// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_active::ActiveAuthority;
use crate::authority_aggregator::AuthorityAggregator;
use crate::authority_client::AuthorityAPI;
use async_trait::async_trait;
use multiaddr::Multiaddr;
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use sui_network::tonic;
use sui_types::committee::Committee;
use sui_types::crypto::PublicKeyBytes;
use sui_types::error::{SuiError, SuiResult};
use sui_types::messages::{ConfirmationTransaction, SignedTransaction};
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use sui_types::sui_system_state::SuiSystemState;
use typed_store::Map;

#[async_trait]
pub trait Reconfigurable {
    fn needs_network_recreation() -> bool;

    fn recreate(channel: tonic::transport::Channel) -> Self;
}

// TODO: Make last checkpoint number of each epoch more flexible.
pub const CHECKPOINT_COUNT_PER_EPOCH: u64 = 200;

const WAIT_BETWEEN_EPOCH_TX_QUERY_RETRY: Duration = Duration::from_millis(300);

impl<A> ActiveAuthority<A>
where
    A: AuthorityAPI + Send + Sync + 'static + Clone + Reconfigurable,
{
    /// This function should be called by the active checkpoint process, when it finishes processing
    /// all transactions from the second to the least checkpoint of the epoch. It's called by a
    /// validator that belongs to the committee of the current epoch.
    pub async fn start_epoch_change(&self) -> SuiResult {
        if let Some(checkpoints) = &self.state.checkpoints {
            let mut checkpoints = checkpoints.lock();
            let next_cp = checkpoints.get_locals().next_checkpoint;
            assert!(
                Self::is_second_last_checkpoint_epoch(next_cp),
                "start_epoch_change called at the wrong checkpoint",
            );

            // drop checkpoints lock
        } else {
            unreachable!();
        }

        self.state.halted.store(true, Ordering::SeqCst);
        while !self.state.batch_notifier.ticket_drained() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok(())
    }

    /// This function should be called by the active checkpoint process, when it finishes processing
    /// all transactions from the last checkpoint of the epoch. This function needs to be called by
    /// a validator that belongs to the committee of the next epoch.
    pub async fn finish_epoch_change(&self) -> SuiResult {
        assert!(
            self.state.halted.load(Ordering::SeqCst),
            "finish_epoch_change called when validator is not halted",
        );
        if let Some(checkpoints) = &self.state.checkpoints {
            let mut checkpoints = checkpoints.lock();
            let next_cp = checkpoints.get_locals().next_checkpoint;
            assert!(
                Self::is_last_checkpoint_epoch(next_cp),
                "finish_epoch_change called at the wrong checkpoint",
            );

            for (tx_digest, _) in checkpoints.extra_transactions.iter() {
                self.state
                    .database
                    .revert_state_update(&tx_digest.transaction)?;
            }

            // Delete any extra certificates now unprocessed.
            checkpoints.extra_transactions.clear()?;

            self.state.database.remove_all_pending_certificates()?;

            // drop checkpoints lock
        } else {
            unreachable!();
        }

        let sui_system_state = self.state.get_sui_system_state_object().await?;
        let next_epoch = sui_system_state.epoch + 1;
        let next_epoch_validators = &sui_system_state.validators.next_epoch_validators;
        let votes = next_epoch_validators
            .iter()
            .map(|metadata| {
                (
                    PublicKeyBytes::try_from(metadata.pubkey_bytes.as_ref())
                        .expect("Validity of public key bytes should be verified on-chain"),
                    metadata.next_epoch_stake,
                )
            })
            .collect();
        let new_committee = Committee::new(next_epoch, votes)?;
        self.state.insert_new_epoch_info(&new_committee)?;

        // Reconnect the network if we have an type of AuthorityClient that has a network.
        if A::needs_network_recreation() {
            self.recreate_network(sui_system_state, new_committee)?;
        } else {
            // update the authorities with the new committee
            let new_net = Arc::new(AuthorityAggregator::new(
                new_committee,
                self.net.load().clone_inner_clients(),
                self.gateway_metrics.clone(),
            ));
            self.net.store(new_net);
        }
        // TODO: Update all committee in all components safely,
        // potentially restart narwhal committee/consensus adapter,
        // all active processes, maybe batch service.
        // We should also reduce the amount of committee passed around.

        let advance_epoch_tx = SignedTransaction::new_change_epoch(
            next_epoch,
            0, // TODO: fill in storage_charge
            0, // TODO: fill in computation_charge
            self.state.name,
            &*self.state.secret,
        );
        // Add the signed transaction to the store.
        self.state
            .set_transaction_lock(&[], advance_epoch_tx.clone())
            .await?;

        // Collect a certificate for this system transaction that changes epoch,
        // and execute it locally.
        loop {
            if let Ok(certificate) = self
                .net
                .load()
                .process_transaction(advance_epoch_tx.clone().to_transaction())
                .await
            {
                self.state
                    .handle_confirmation_transaction(ConfirmationTransaction { certificate })
                    .await
                    .expect("Executing the special cert cannot fail");
                break;
            }

            tokio::time::sleep(WAIT_BETWEEN_EPOCH_TX_QUERY_RETRY).await;
        }

        // Resume the validator to start accepting transactions for the new epoch.
        self.state.unhalt_validator()?;
        Ok(())
    }

    pub fn is_last_checkpoint_epoch(checkpoint: CheckpointSequenceNumber) -> bool {
        checkpoint > 0 && checkpoint % CHECKPOINT_COUNT_PER_EPOCH == 0
    }

    pub fn is_second_last_checkpoint_epoch(checkpoint: CheckpointSequenceNumber) -> bool {
        (checkpoint + 1) % CHECKPOINT_COUNT_PER_EPOCH == 0
    }

    /// Recreates the network if the client is a type of client that has a network, and swap the new
    /// clients onto the authority aggregator with the new committee.
    pub fn recreate_network(
        &self,
        sui_system_state: SuiSystemState,
        new_committee: Committee,
    ) -> SuiResult {
        let mut new_clients = BTreeMap::new();
        let next_epoch_validators = sui_system_state.validators.next_epoch_validators;

        let mut net_config = mysten_network::config::Config::new();
        net_config.connect_timeout = Some(Duration::from_secs(5));
        net_config.request_timeout = Some(Duration::from_secs(5));
        net_config.http2_keepalive_interval = Some(Duration::from_secs(5));

        for validator in next_epoch_validators {
            let net_addr: &[u8] = &validator.net_address.clone();
            let str_addr =
                std::str::from_utf8(net_addr).map_err(|e| SuiError::GenericAuthorityError {
                    error: e.to_string(),
                });
            let address: Multiaddr = str_addr
                .unwrap()
                .parse()
                .map_err(|e: multiaddr::Error| SuiError::GenericAuthorityError {
                    error: e.to_string(),
                })
                .unwrap();

            let channel = net_config
                .connect_lazy(&address)
                .map_err(|e| SuiError::GenericAuthorityError {
                    error: e.to_string(),
                })
                .unwrap();
            let client: A = A::recreate(channel);
            let name: &[u8] = &validator.name;
            let public_key_bytes = PublicKeyBytes::try_from(name)?;
            new_clients.insert(public_key_bytes, client);
        }

        // Replace the clients in the authority aggregator with new clients.
        let new_net = Arc::new(AuthorityAggregator::new(
            new_committee,
            new_clients,
            self.gateway_metrics.clone(),
        ));
        self.net.store(new_net);
        Ok(())
    }
}
