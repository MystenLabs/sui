// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_active::ActiveAuthority;
use std::sync::atomic::Ordering;
use std::time::Duration;
use sui_types::committee::Committee;
use sui_types::crypto::PublicKeyBytes;
use sui_types::error::{SuiResult, SuiError};
use sui_types::fp_ensure;
use sui_types::messages::SignedTransaction;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use tokio::time::Instant;
use typed_store::Map;

// TODO: Make last checkpoint number of each epoch more flexible.
const CHECKPOINT_COUNT_PER_EPOCH: u64 = 200;

const MAX_START_EPOCH_WAIT_SECONDS: Duration = Duration::from_secs(5);

impl<A> ActiveAuthority<A> {
    pub async fn start_epoch_change(&mut self) -> SuiResult {
        {
            let checkpoints = self.state.checkpoints.as_ref().unwrap().lock();
            let next_cp = checkpoints.get_locals().next_checkpoint;
            fp_ensure!(
                Self::is_second_last_checkpoint_epoch(next_cp),
                SuiError::InconsistentEpochState {
                    error: "start_epoch_change called at the wrong checkpoint".to_owned(),
                }
            );
            fp_ensure!(
                checkpoints.lowest_unprocessed_checkpoint() == next_cp,
                SuiError::InconsistentEpochState {
                    error: "start_epoch_change called when there are still unprocessed transactions".to_owned(),
                }
            );
            // drop checkpoints lock
        }
        
        self.state.halted.store(true, Ordering::SeqCst);
        let instant = Instant::now();
        while !self.state.batch_notifier.ticket_drained() {
            tokio::time::sleep(Duration::from_millis(50)).await;
            fp_ensure!(
                instant.elapsed() <= MAX_START_EPOCH_WAIT_SECONDS,
                SuiError::InconsistentEpochState {
                    error: "Waiting for batch_notifier ticket to drain timed out in start_epoch_change".to_owned(),
                }
            );
        }
        Ok(())
    }

    pub async fn finish_epoch_change(&mut self) -> SuiResult {
        fp_ensure!(
            self.state.halted.load(Ordering::SeqCst),
            SuiError::InconsistentEpochState {
                error: "finish_epoch_change called when validator is not halted".to_owned(),
            }
        );
        {
            let checkpoints = self.state.checkpoints.as_ref().unwrap().lock();
            let next_cp = checkpoints.get_locals().next_checkpoint;
            fp_ensure!(
                Self::is_last_checkpoint_epoch(next_cp),
                SuiError::InconsistentEpochState {
                    error: "finish_epoch_change called at the wrong checkpoint".to_owned(),
                }
            );
            fp_ensure!(
                checkpoints.lowest_unprocessed_checkpoint() == next_cp,
                SuiError::InconsistentEpochState {
                    error: "finish_epoch_change called when there are still unprocessed transactions".to_owned(),
                }
            );
            if checkpoints.extra_transactions.iter().next().is_some() {
                // TODO: Revert any tx that's executed but not in the checkpoint.
            }
            // drop checkpoints lock
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
        let new_committee = Committee::new(next_epoch, votes);
        self.state.insert_new_epoch_info(&new_committee)?;
        self.state.checkpoints.as_ref().unwrap().lock().committee = new_committee;
        // TODO: Update all committee in all components, potentially restart some authority clients.
        // Including: self.net, narwhal committee, anything else?
        // We should also reduce the amount of committee passed around.

        let advance_epoch_tx = SignedTransaction::new_change_epoch(
            next_epoch,
            0, // TODO: fill in storage_charge
            0, // TODO: fill in computation_charge
            self.state.name,
            &*self.state.secret,
        );
        self.state
            .change_epoch_tx
            .lock()
            .insert(self.state.name, advance_epoch_tx);

        // TODO: Now ask every validator in the committee for this signed tx.
        // Aggregate them to obtain a cert, execute the cert, and then start the new epoch.

        self.state.begin_new_epoch()?;
        Ok(())
    }

    fn is_last_checkpoint_epoch(checkpoint: CheckpointSequenceNumber) -> bool {
        checkpoint > 0 && checkpoint % CHECKPOINT_COUNT_PER_EPOCH == 0
    }

    fn is_second_last_checkpoint_epoch(checkpoint: CheckpointSequenceNumber) -> bool {
        (checkpoint + 1) % CHECKPOINT_COUNT_PER_EPOCH == 0
    }
}
