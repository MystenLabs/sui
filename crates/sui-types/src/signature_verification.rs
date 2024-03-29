// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::base_types::SuiAddress;
use crate::error::{SuiError, SuiResult, UserInputError};
use crate::signature::{AuthenticatorTrait, GenericSignature, VerifyParams};
use crate::transaction::{
    SenderSignedData, SenderSignedTransaction, TransactionData, TransactionDataAPI,
};
use shared_crypto::intent::{Intent, IntentMessage, IntentScope};

pub fn verify_sender_signed_data_user_input(txn: &SenderSignedData) -> SuiResult {
    fp_ensure!(
        txn.inner_transactions().len() == 1,
        SuiError::UserInputError {
            error: UserInputError::Unsupported(
                "SenderSignedData must contain exactly one transaction".to_string()
            )
        }
    );
    let tx_data = &txn.intent_message().value;
    fp_ensure!(
        !tx_data.is_system_tx(),
        SuiError::UserInputError {
            error: UserInputError::Unsupported(
                "SenderSignedData must not contain system transaction".to_string()
            )
        }
    );

    // Verify signatures are well formed. Steps are ordered in asc complexity order
    // to minimize abuse.
    let signers = txn.intent_message().value.signers();
    // Signature number needs to match
    fp_ensure!(
        txn.inner().tx_signatures.len() == signers.len(),
        SuiError::SignerSignatureNumberMismatch {
            actual: txn.inner().tx_signatures.len(),
            expected: signers.len()
        }
    );

    // All required signers need to be sign.
    let present_sigs = txn.get_signer_sig_mapping(true)?;
    for s in signers {
        if !present_sigs.contains_key(&s) {
            return Err(SuiError::SignerSignatureAbsent {
                expected: s.to_string(),
                actual: present_sigs.keys().map(|s| s.to_string()).collect(),
            });
        }
    }

    Ok(())
}

pub fn verify_sender_signed_data_message_signatures(txn: &SenderSignedTransaction) -> SuiResult {
    if txn.intent_message.value.is_system_tx() {
        return Ok(());
    }

    // Verify signatures. Steps are ordered in asc complexity order to minimize abuse.
    let signers = txn.intent_message.value.signers();
    // Signature number needs to match
    fp_ensure!(
        txn.tx_signatures.len() == signers.len(),
        SuiError::SignerSignatureNumberMismatch {
            actual: txn.tx_signatures.len(),
            expected: signers.len()
        }
    );
    // All required signers need to be sign.
    let present_sigs = txn.get_signer_sig_mapping(verify_params.verify_legacy_zklogin_address)?;
    for s in signers {
        if !present_sigs.contains_key(&s) {
            return Err(SuiError::SignerSignatureAbsent {
                expected: s.to_string(),
                actual: present_sigs.keys().map(|s| s.to_string()).collect(),
            });
        }
    }

    // Verify all present signatures.
    for (signer, signature) in present_sigs {
        verify_signature(txn.intent_message(), signature, signer, verify_params)?;
    }
    Ok(())
}

fn verify_signature(
    intent_message: &IntentMessage<TransactionData>,
    signature: &GenericSignature,
    signer: SuiAddress,
    verify_params: &VerifyParams,
) -> SuiResult {
    match signature {
        GenericSignature::MultiSig(multisig) => (),
        GenericSignature::MultiSigLegacy(multisig_legacy) => (),
        GenericSignature::Signature(sig) => (),
        GenericSignature::ZkLoginAuthenticator(zk_login_authenticator) => (),
    }

    Ok(())
}
