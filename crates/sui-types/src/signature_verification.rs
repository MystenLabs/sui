// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;

use crate::base_types::SuiAddress;
use crate::committee::EpochId;
use crate::error::{SuiError, SuiResult, UserInputError};
use crate::signature::{GenericSignature, VerifyParams};
use crate::transaction::{SenderSignedData, TransactionDataAPI};

enum RejectSystemTransactions {
    Yes,
    No,
}

impl RejectSystemTransactions {
    fn reject(&self) -> bool {
        matches!(self, RejectSystemTransactions::Yes)
    }
}

fn verify_sender_signed_data_user_input_and_return_sig_map(
    txn: &SenderSignedData,
    reject_system_txns: RejectSystemTransactions,
) -> SuiResult<BTreeMap<SuiAddress, &GenericSignature>> {
    // 1. SenderSignedData must contain exactly one transaction.
    fp_ensure!(
        txn.inner_transactions().len() == 1,
        SuiError::UserInputError {
            error: UserInputError::Unsupported(
                "SenderSignedData must contain exactly one transaction".to_string()
            )
        }
    );

    if reject_system_txns.reject() {
        // 2. Users cannot send system transactions.
        let tx_data = &txn.intent_message().value;
        fp_ensure!(
            !tx_data.is_system_tx(),
            SuiError::UserInputError {
                error: UserInputError::Unsupported(
                    "SenderSignedData must not contain system transaction".to_string()
                )
            }
        );
    }

    // 3. One signature per sender must be present. (signers is a NonEmpty vec so >0 sigs is ensured).
    let signers = txn.intent_message().value.signers();
    fp_ensure!(
        txn.inner().tx_signatures.len() == signers.len(),
        SuiError::SignerSignatureNumberMismatch {
            actual: txn.inner().tx_signatures.len(),
            expected: signers.len()
        }
    );

    // 4. Each signer must provide a signature.
    let present_sigs = txn.get_signer_sig_mapping(true)?;
    for s in signers {
        if !present_sigs.contains_key(&s) {
            return Err(SuiError::SignerSignatureAbsent {
                expected: s.to_string(),
                actual: present_sigs.keys().map(|s| s.to_string()).collect(),
            });
        }
    }

    Ok(present_sigs)
}

/// Does all non-crypto validation for a user-provided transaction.
pub fn verify_sender_signed_data_user_input(txn: &SenderSignedData) -> SuiResult {
    verify_sender_signed_data_user_input_and_return_sig_map(
        txn,
        RejectSystemTransactions::Yes, // user input may not contain system transactions
    )?;
    Ok(())
}

/// Does crypto validation for a transaction which may be user-provided, or may be from a checkpoint.
pub fn verify_sender_signed_data_message_signatures(
    txn: &SenderSignedData,
    current_epoch: EpochId,
    verify_params: &VerifyParams,
) -> SuiResult {
    // 1. Validate user input.
    // TODO: Use types to enforce that verify_sender_signed_data_user_input is called first
    // instead of doing this check twice. This check is very cheap however so doing it twice
    // is not a big deal.
    let present_sigs = verify_sender_signed_data_user_input_and_return_sig_map(
        txn,
        RejectSystemTransactions::No, // system transactions are allowed when verifying from checkpoint
    )?;

    let intent_message = &txn.inner().intent_message;

    // 2. System transactions (from checkpoints) do not required signatures.
    if intent_message.value.is_system_tx() {
        return Ok(());
    }

    // 3. Every signature must be valid.
    for (signer, signature) in present_sigs {
        signature.verify_authenticator(intent_message, signer, current_epoch, verify_params)?;
    }
    Ok(())
}
