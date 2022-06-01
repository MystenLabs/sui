// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::authority_active::ActiveAuthority;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use sui_types::committee::Committee;
use sui_types::crypto::PublicKeyBytes;
use sui_types::error::SuiResult;
use sui_types::messages::SignedTransaction;
use sui_types::messages_checkpoint::CheckpointSequenceNumber;
use typed_store::Map;

// TODO: Make last checkpoint number of each epoch more flexible.
pub const CHECKPOINT_COUNT_PER_EPOCH: u64 = 200;

impl<A> ActiveAuthority<A> {
    pub async fn start_epoch_change(&self) -> SuiResult {
        if let Some(checkpoints) = &self.state.checkpoints {
            let mut checkpoints = checkpoints.lock();
            let next_cp = checkpoints.get_locals().next_checkpoint;
            assert!(
                Self::is_second_last_checkpoint_epoch(next_cp),
                "start_epoch_change called at the wrong checkpoint",
            );
            assert_eq!(
                checkpoints.lowest_unprocessed_checkpoint(),
                next_cp,
                "start_epoch_change called when there are still unprocessed transactions",
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
            assert_eq!(
                checkpoints.lowest_unprocessed_checkpoint(),
                next_cp,
                "finish_epoch_change called when there are still unprocessed transactions",
            );
            if checkpoints.extra_transactions.iter().next().is_some() {
                // TODO: Revert any tx that's executed but not in the checkpoint.
            }
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
        let new_committee = Committee::new(next_epoch, votes);
        self.state.insert_new_epoch_info(&new_committee)?;
        //self.state.checkpoints.as_ref().unwrap().lock().committee = new_committee;
        // TODO: Update all committee in all components safely,
        // potentially restart some authority clients.
        // Including: self.net, narwhal committee/consensus adapter,
        // all active processes, maybe batch service.
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
            .store(Some(Arc::new(advance_epoch_tx)));

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
