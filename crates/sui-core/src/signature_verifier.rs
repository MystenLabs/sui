// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use fastcrypto_zkp::bn254::zk_login::JwkId;
use fastcrypto_zkp::bn254::zk_login::{JWK, OIDCProvider};
use fastcrypto_zkp::bn254::zk_login_api::ZkLoginEnv;
use im::hashmap::HashMap as ImHashMap;
use itertools::Itertools as _;
use mysten_common::debug_fatal;
use nonempty::NonEmpty;
use parking_lot::RwLock;
use prometheus::{IntCounter, Registry, register_int_counter_with_registry};
use shared_crypto::intent::Intent;
use std::sync::Arc;
use sui_types::address_alias;
use sui_types::base_types::{SequenceNumber, SuiAddress};
use sui_types::digests::SenderSignedDataDigest;
use sui_types::digests::ZKLoginInputsDigest;
use sui_types::signature_verification::{
    VerifiedDigestCache, verify_sender_signed_data_message_signatures,
};
use sui_types::storage::ObjectStore;
use sui_types::transaction::{SenderSignedData, TransactionDataAPI};
use sui_types::{
    committee::Committee,
    crypto::{AuthoritySignInfoTrait, VerificationObligation},
    digests::CertificateDigest,
    error::{SuiErrorKind, SuiResult},
    message_envelope::Message,
    messages_checkpoint::SignedCheckpointSummary,
    signature::VerifyParams,
    transaction::CertifiedTransaction,
};
use tracing::debug;

/// Verifies signatures in ways that are faster than verifying each signature individually.
/// - BLS signatures - caching and batch verification.
/// - User signed data - caching.
pub struct SignatureVerifier {
    committee: Arc<Committee>,
    object_store: Arc<dyn ObjectStore + Send + Sync>,
    certificate_cache: VerifiedDigestCache<CertificateDigest>,
    signed_data_cache: VerifiedDigestCache<SenderSignedDataDigest, Vec<u8>>,
    zklogin_inputs_cache: Arc<VerifiedDigestCache<ZKLoginInputsDigest>>,

    /// Map from JwkId (iss, kid) to the fetched JWK for that key.
    /// We use an immutable data structure because verification of ZKLogins may be slow, so we
    /// don't want to pass a reference to the map to the verify method, since that would lead to a
    /// lengthy critical section. Instead, we use an immutable data structure which can be cloned
    /// very cheaply.
    jwks: RwLock<ImHashMap<JwkId, JWK>>,

    /// Params that contains a list of supported providers for ZKLogin and the environment (prod/test) the code runs in.
    zk_login_params: ZkLoginParams,

    /// If true, uses address aliases during signature verification.
    enable_address_aliases: bool,

    pub metrics: Arc<SignatureVerifierMetrics>,
}

/// Contains two parameters to pass in to verify a ZkLogin signature.
#[derive(Clone)]
struct ZkLoginParams {
    /// A list of supported OAuth providers for ZkLogin.
    pub supported_providers: Vec<OIDCProvider>,
    /// The environment (prod/test) the code runs in. It decides which verifying key to use in fastcrypto.
    pub env: ZkLoginEnv,
    /// Flag to determine whether legacy address (derived from padded address seed) should be verified.
    pub verify_legacy_zklogin_address: bool,
    // Flag to determine whether zkLogin inside multisig is accepted.
    pub accept_zklogin_in_multisig: bool,
    // Flag to determine whether passkey inside multisig is accepted.
    pub accept_passkey_in_multisig: bool,
    /// Value that sets the upper bound for max_epoch in zkLogin signature.
    pub zklogin_max_epoch_upper_bound_delta: Option<u64>,
    /// Flag to determine whether additional multisig checks are performed.
    pub additional_multisig_checks: bool,
    /// Flag to determine whether additional zkLogin public identifier structure is validated.
    pub validate_zklogin_public_identifier: bool,
}

impl SignatureVerifier {
    pub fn new(
        committee: Arc<Committee>,
        object_store: Arc<dyn ObjectStore + Send + Sync>,
        metrics: Arc<SignatureVerifierMetrics>,
        supported_providers: Vec<OIDCProvider>,
        zklogin_env: ZkLoginEnv,
        verify_legacy_zklogin_address: bool,
        accept_zklogin_in_multisig: bool,
        accept_passkey_in_multisig: bool,
        zklogin_max_epoch_upper_bound_delta: Option<u64>,
        additional_multisig_checks: bool,
        validate_zklogin_public_identifier: bool,
        enable_address_aliases: bool,
    ) -> Self {
        Self {
            committee,
            object_store,
            certificate_cache: VerifiedDigestCache::new(
                metrics.certificate_signatures_cache_hits.clone(),
                metrics.certificate_signatures_cache_misses.clone(),
                metrics.certificate_signatures_cache_evictions.clone(),
            ),
            signed_data_cache: VerifiedDigestCache::new(
                metrics.signed_data_cache_hits.clone(),
                metrics.signed_data_cache_misses.clone(),
                metrics.signed_data_cache_evictions.clone(),
            ),
            zklogin_inputs_cache: Arc::new(VerifiedDigestCache::new(
                metrics.zklogin_inputs_cache_hits.clone(),
                metrics.zklogin_inputs_cache_misses.clone(),
                metrics.zklogin_inputs_cache_evictions.clone(),
            )),
            jwks: Default::default(),
            enable_address_aliases,
            metrics,
            zk_login_params: ZkLoginParams {
                supported_providers,
                env: zklogin_env,
                verify_legacy_zklogin_address,
                accept_zklogin_in_multisig,
                accept_passkey_in_multisig,
                zklogin_max_epoch_upper_bound_delta,
                additional_multisig_checks,
                validate_zklogin_public_identifier,
            },
        }
    }

    /// Verifies all certs, returns Ok only if all are valid.
    pub fn verify_certs_and_checkpoints(
        &self,
        certs: Vec<&CertifiedTransaction>,
        checkpoints: Vec<&SignedCheckpointSummary>,
    ) -> SuiResult {
        let certs: Vec<_> = certs
            .into_iter()
            .filter(|cert| !self.certificate_cache.is_cached(&cert.certificate_digest()))
            .collect();

        // Verify only the user sigs of certificates that were not cached already, since whenever we
        // insert a certificate into the cache, it is already verified.
        // Aliases are only allowed via MFP, so CertifiedTransaction must have no aliases.
        for cert in &certs {
            self.verify_tx_require_no_aliases(cert.data())?;
        }
        batch_verify_all_certificates_and_checkpoints(&self.committee, &certs, &checkpoints)?;
        self.certificate_cache
            .cache_digests(certs.into_iter().map(|c| c.certificate_digest()).collect());
        Ok(())
    }

    /// Insert a JWK into the verifier state. Pre-existing entries for a given JwkId will not be
    /// overwritten.
    pub(crate) fn insert_jwk(&self, jwk_id: &JwkId, jwk: &JWK) {
        let mut jwks = self.jwks.write();
        match jwks.entry(jwk_id.clone()) {
            im::hashmap::Entry::Occupied(_) => {
                debug!("JWK with kid {:?} already exists", jwk_id);
            }
            im::hashmap::Entry::Vacant(entry) => {
                debug!("inserting JWK with kid: {:?}", jwk_id);
                entry.insert(jwk.clone());
            }
        }
    }

    pub fn has_jwk(&self, jwk_id: &JwkId, jwk: &JWK) -> bool {
        let jwks = self.jwks.read();
        jwks.get(jwk_id) == Some(jwk)
    }

    pub fn get_jwks(&self) -> ImHashMap<JwkId, JWK> {
        self.jwks.read().clone()
    }

    // For each required signer in the transaction, returns the signature index and
    // version of the AddressAliases object used to verify it.
    pub fn verify_tx_with_current_aliases(
        &self,
        signed_tx: &SenderSignedData,
    ) -> SuiResult<NonEmpty<(u8, Option<SequenceNumber>)>> {
        let mut alias_versions_by_signer = Vec::new();
        let mut aliases = Vec::new();

        // Look up aliases for each address at the current version.
        let signers = signed_tx.intent_message().value.required_signers();
        for signer in signers {
            if !self.enable_address_aliases {
                alias_versions_by_signer.push((signer, None));
                aliases.push((signer, NonEmpty::singleton(signer)));
            } else {
                // Look up aliases for the signer using the derived object address.
                let address_aliases =
                    address_alias::get_address_aliases_from_store(&self.object_store, signer)?;

                alias_versions_by_signer.push((signer, address_aliases.as_ref().map(|(_, v)| *v)));
                aliases.push((
                    signer,
                    address_aliases
                        .map(|(aliases, _)| {
                            NonEmpty::from_vec(aliases.aliases.contents.clone()).unwrap_or_else(
                                || {
                                    debug_fatal!(
                                    "AddressAliases struct has empty aliases field for signer {}",
                                    signer
                                );
                                    NonEmpty::singleton(signer)
                                },
                            )
                        })
                        .unwrap_or(NonEmpty::singleton(signer)),
                ));
            }
        }

        // Verify and get the signature indices for each required signer.
        let sig_indices = self.verify_tx(signed_tx, &alias_versions_by_signer, aliases)?;

        // Combine signature indices with alias versions.
        let result: Vec<(u8, Option<SequenceNumber>)> = sig_indices
            .into_iter()
            .zip_eq(alias_versions_by_signer.into_iter().map(|(_, seq)| seq))
            .collect();

        Ok(NonEmpty::from_vec(result).expect("must have at least one required_signer"))
    }

    pub fn verify_tx_require_no_aliases(&self, signed_tx: &SenderSignedData) -> SuiResult {
        let current_aliases = self.verify_tx_with_current_aliases(signed_tx)?;
        for (_, version) in current_aliases {
            if version.is_some() {
                return Err(SuiErrorKind::AliasesChanged.into());
            }
        }
        Ok(())
    }

    fn verify_tx(
        &self,
        signed_tx: &SenderSignedData,
        alias_versions: &Vec<(SuiAddress, Option<SequenceNumber>)>,
        aliased_addresses: Vec<(SuiAddress, NonEmpty<SuiAddress>)>,
    ) -> SuiResult<Vec<u8>> {
        let digest = signed_tx.full_message_digest_with_alias_versions(alias_versions);

        if let Some(indices) = self.signed_data_cache.get_cached(&digest) {
            return Ok(indices);
        }

        let jwks = self.jwks.read().clone();
        let verify_params = VerifyParams::new(
            jwks,
            self.zk_login_params.supported_providers.clone(),
            self.zk_login_params.env,
            self.zk_login_params.verify_legacy_zklogin_address,
            self.zk_login_params.accept_zklogin_in_multisig,
            self.zk_login_params.accept_passkey_in_multisig,
            self.zk_login_params.zklogin_max_epoch_upper_bound_delta,
            self.zk_login_params.additional_multisig_checks,
            self.zk_login_params.validate_zklogin_public_identifier,
        );
        let indices = verify_sender_signed_data_message_signatures(
            signed_tx,
            self.committee.epoch(),
            &verify_params,
            self.zklogin_inputs_cache.clone(),
            aliased_addresses,
        )?;

        self.signed_data_cache
            .cache_with_value(digest, indices.clone());
        Ok(indices)
    }

    pub fn clear_signature_cache(&self) {
        self.certificate_cache.clear();
        self.signed_data_cache.clear();
        self.zklogin_inputs_cache.clear();
    }
}

pub struct SignatureVerifierMetrics {
    pub certificate_signatures_cache_hits: IntCounter,
    pub certificate_signatures_cache_misses: IntCounter,
    pub certificate_signatures_cache_evictions: IntCounter,
    pub signed_data_cache_hits: IntCounter,
    pub signed_data_cache_misses: IntCounter,
    pub signed_data_cache_evictions: IntCounter,
    pub zklogin_inputs_cache_hits: IntCounter,
    pub zklogin_inputs_cache_misses: IntCounter,
    pub zklogin_inputs_cache_evictions: IntCounter,
}

impl SignatureVerifierMetrics {
    pub fn new(registry: &Registry) -> Arc<Self> {
        Arc::new(Self {
            certificate_signatures_cache_hits: register_int_counter_with_registry!(
                "certificate_signatures_cache_hits",
                "Number of certificates which were known to be verified because of signature cache.",
                registry
            )
            .unwrap(),
            certificate_signatures_cache_misses: register_int_counter_with_registry!(
                "certificate_signatures_cache_misses",
                "Number of certificates which missed the signature cache",
                registry
            )
            .unwrap(),
            certificate_signatures_cache_evictions: register_int_counter_with_registry!(
                "certificate_signatures_cache_evictions",
                "Number of times we evict a pre-existing key were known to be verified because of signature cache.",
                registry
            )
            .unwrap(),
            signed_data_cache_hits: register_int_counter_with_registry!(
                "signed_data_cache_hits",
                "Number of signed data which were known to be verified because of signature cache.",
                registry
            )
            .unwrap(),
            signed_data_cache_misses: register_int_counter_with_registry!(
                "signed_data_cache_misses",
                "Number of signed data which missed the signature cache.",
                registry
            )
            .unwrap(),
            signed_data_cache_evictions: register_int_counter_with_registry!(
                "signed_data_cache_evictions",
                "Number of times we evict a pre-existing signed data were known to be verified because of signature cache.",
                registry
            )
                .unwrap(),
                zklogin_inputs_cache_hits: register_int_counter_with_registry!(
                    "zklogin_inputs_cache_hits",
                    "Number of zklogin signature which were known to be partially verified because of zklogin inputs cache.",
                    registry
                )
                .unwrap(),
                zklogin_inputs_cache_misses: register_int_counter_with_registry!(
                    "zklogin_inputs_cache_misses",
                    "Number of zklogin signatures which missed the zklogin inputs cache.",
                    registry
                )
                .unwrap(),
                zklogin_inputs_cache_evictions: register_int_counter_with_registry!(
                    "zklogin_inputs_cache_evictions",
                    "Number of times we evict a pre-existing zklogin inputs digest that was known to be verified because of zklogin inputs cache.",
                    registry
                )
                .unwrap(),
        })
    }
}

/// Verifies all certificates - if any fail return error.
pub(crate) fn batch_verify_all_certificates_and_checkpoints(
    committee: &Committee,
    certs: &[&CertifiedTransaction],
    checkpoints: &[&SignedCheckpointSummary],
) -> SuiResult {
    // certs.data() is assumed to be verified already by the caller.

    for ckpt in checkpoints {
        ckpt.data().verify_epoch(committee.epoch())?;
    }

    batch_verify(committee, certs, checkpoints)
}

fn batch_verify(
    committee: &Committee,
    certs: &[&CertifiedTransaction],
    checkpoints: &[&SignedCheckpointSummary],
) -> SuiResult {
    let mut obligation = VerificationObligation::default();

    for cert in certs {
        let idx = obligation.add_message(cert.data(), cert.epoch(), Intent::sui_app(cert.scope()));
        cert.auth_sig()
            .add_to_verification_obligation(committee, &mut obligation, idx)?;
    }

    for ckpt in checkpoints {
        let idx = obligation.add_message(ckpt.data(), ckpt.epoch(), Intent::sui_app(ckpt.scope()));
        ckpt.auth_sig()
            .add_to_verification_obligation(committee, &mut obligation, idx)?;
    }

    obligation.verify_all()
}
