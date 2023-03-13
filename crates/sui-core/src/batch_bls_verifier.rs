// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use shared_crypto::intent::{Intent, IntentScope};
use sui_types::{
    committee::Committee,
    crypto::{AuthoritySignInfoTrait, VerificationObligation},
    error::SuiResult,
    message_envelope::Message,
    messages::CertifiedTransaction,
};

use tap::TapFallible;
use tracing::error;

/// Verifies all certificates - if any fail return error.
pub fn batch_verify_all_certificates(
    committee: &Committee,
    certs: &[CertifiedTransaction],
) -> SuiResult {
    // Verify user signatures
    for cert in certs {
        cert.data().verify(None)?;
    }

    batch_verify_certificates_impl(committee, certs)
}

/// Verifies certificates in batch mode, but returns a separate result for each cert.
pub fn batch_verify_certificates(
    committee: &Committee,
    certs: &[CertifiedTransaction],
) -> Vec<SuiResult> {
    match batch_verify_certificates_impl(committee, certs) {
        Ok(_) => certs
            .iter()
            .map(|c| {
                c.data().verify(None).tap_err(|e| {
                    error!(
                        "Cert was signed by quorum, but contained bad user signatures! {}",
                        e
                    )
                })?;
                Ok(())
            })
            .collect(),

        // Verify one by one to find which certs were invalid.
        Err(_) if certs.len() > 1 => certs
            .iter()
            .map(|c| c.verify_signature(committee))
            .collect(),

        Err(e) => vec![Err(e)],
    }
}

fn batch_verify_certificates_impl(
    committee: &Committee,
    certs: &[CertifiedTransaction],
) -> SuiResult {
    let mut obligation = VerificationObligation::default();

    for cert in certs {
        let idx = obligation.add_message(
            cert.data(),
            cert.epoch(),
            Intent::default().with_scope(IntentScope::SenderSignedTransaction),
        );
        cert.auth_sig()
            .add_to_verification_obligation(committee, &mut obligation, idx)?;
    }

    obligation.verify_all()
}
