// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::*;

// Here we gather the crypto related commands, and also provide mocks for them
// so that we can turn off crypto when we benchmark to detect hotspots in other
// parts of the execution. The turning off is done through a compilation flag
// to ensure this is not done in produce, and turning off verification also turns
// off signing to ensure we never can emit bad signatures.

// The real crypto is here
#[cfg(not(feature = "mockcrypto"))]
impl AuthorityState {
    pub(crate) fn check_certificate(&self, certificate: &CertifiedTransaction) -> SuiResult<()> {
        certificate.check(&self.committee)
    }

    pub(crate) fn check_transaction(&self, transaction: &Transaction) -> SuiResult<()> {
        transaction.check_signature()
    }

    pub(crate) fn provision_secret(secret: StableSyncAuthoritySigner) -> StableSyncAuthoritySigner {
        secret
    }
}

/*      The mock crypto is here

        Profile with:
        $ CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph --no-inline --features mockcrypto --bin=bench -- --num-accounts 160000 --max-in-flight 3200 --use-move --db-cpus 4

*/
#[cfg(feature = "mockcrypto")]
impl AuthorityState {
    pub(crate) fn check_certificate(&self, _certificate: &CertifiedTransaction) -> SuiResult<()> {
        Ok(())
    }

    pub(crate) fn check_transaction(&self, _transaction: &Transaction) -> SuiResult<()> {
        Ok(())
    }

    pub(crate) fn provision_secret(secret: StableSyncAuthoritySigner) -> StableSyncAuthoritySigner {
        use sui_types::crypto::MockNoopSigner;
        let one_signature = secret.try_sign(&(vec![1, 2, 3])[..]).unwrap();
        Arc::pin(MockNoopSigner(one_signature))
    }
}
