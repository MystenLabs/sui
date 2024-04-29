// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use nonempty::NonEmpty;
use shared_crypto::intent::Intent;

use crate::committee::EpochId;
use crate::error::{SuiError, SuiResult};
use crate::signature::VerifyParams;
use crate::transaction::{SenderSignedData, TransactionDataAPI};

/// Does crypto validation for a transaction which may be user-provided, or may be from a checkpoint.
pub fn verify_sender_signed_data_message_signatures(
    txn: &SenderSignedData,
    current_epoch: EpochId,
    verify_params: &VerifyParams,
) -> SuiResult {
    let intent_message = txn.intent_message();
    assert_eq!(intent_message.intent, Intent::sui_transaction());

    // 1. System transactions do not require signatures. User-submitted transactions are verified not to
    // be system transactions before this point
    if intent_message.value.is_system_tx() {
        return Ok(());
    }

    // 2. One signature per signer is required.
    let signers: NonEmpty<_> = txn.intent_message().value.signers();
    fp_ensure!(
        txn.inner().tx_signatures.len() == signers.len(),
        SuiError::SignerSignatureNumberMismatch {
            actual: txn.inner().tx_signatures.len(),
            expected: signers.len()
        }
    );

    // 3. Each signer must provide a signature.
    let present_sigs = txn.get_signer_sig_mapping(verify_params.verify_legacy_zklogin_address)?;
    for s in signers {
        if !present_sigs.contains_key(&s) {
            return Err(SuiError::SignerSignatureAbsent {
                expected: s.to_string(),
                actual: present_sigs.keys().map(|s| s.to_string()).collect(),
            });
        }
    }

    // 4. Every signature must be valid.
    for (signer, signature) in present_sigs {
        signature.verify_authenticator(intent_message, signer, current_epoch, verify_params)?;
    }
    Ok(())
}
