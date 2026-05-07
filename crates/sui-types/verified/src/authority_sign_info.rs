// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Definition of `AuthoritySignInfo` and its Verus specifications.
//!
//! The type lives here so that Verus can attach spec functions to it as
//! first-class verified code.  `sui-types` re-exports it so all existing
//! imports are unaffected.
//!
//! `sui-types-verified` does NOT depend on `sui-types` to avoid a cycle;
//! only `fastcrypto` and `shared_crypto` are needed for the exec impls.

use fastcrypto::bls12381::min_sig::BLS12381Signature;
use fastcrypto::traits::Signer;
use serde::{Deserialize, Serialize};
use shared_crypto::intent::{Intent, IntentMessage};
use std::fmt::{Display, Formatter};
use std::hash::{Hash, Hasher};
use vstd::prelude::*;

use crate::authority_name::AuthorityPublicKeyBytes;

// AuthorityName is an alias for AuthorityPublicKeyBytes in sui-types;
// we use the concrete type here since we can't depend on sui-types.
pub type AuthorityName = AuthorityPublicKeyBytes;

// Type aliases matching sui-types::crypto.
pub type EpochId = u64;
pub type AuthoritySignature = BLS12381Signature;

/// A single authority's signature together with its epoch and identity.
///
/// Defined here (in the verified crate) so that Verus can attach spec
/// functions — in particular `sig_is_valid` — without orphan issues.
/// `sui-types` re-exports this type so all existing imports are unchanged.
#[derive(Clone, Debug, Eq, Serialize, Deserialize)]
pub struct AuthoritySignInfo {
    pub epoch: EpochId,
    pub authority: AuthorityName,
    pub signature: AuthoritySignature,
}

impl AuthoritySignInfo {
    // Getter methods used by assume_specification below to expose the
    // epoch and authority fields in spec without direct field access
    // (which is disallowed on types defined outside `verus! { }`).
    pub fn get_epoch(&self) -> EpochId {
        self.epoch
    }
    pub fn get_authority(&self) -> AuthorityName {
        self.authority
    }

    pub fn new<T: Serialize>(
        epoch: EpochId,
        value: &T,
        intent: Intent,
        name: AuthorityName,
        secret: &dyn Signer<AuthoritySignature>,
    ) -> Self {
        // Replicates sui-types::crypto::SuiAuthoritySignature::new_secure:
        //   1. BCS-serialize the IntentMessage (no type-name prefix for this step).
        //   2. Append BCS-encoded epoch (EpochId::write also has no prefix).
        //   3. Sign the concatenated bytes.
        let intent_msg = IntentMessage::new(intent, value);
        let mut bytes = bcs::to_bytes(&intent_msg).expect("Message serialization should not fail");
        bcs::serialize_into(&mut bytes, &epoch).expect("Epoch serialization should not fail");
        Self {
            epoch,
            authority: name,
            signature: secret.sign(&bytes),
        }
    }
}

impl Hash for AuthoritySignInfo {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.epoch.hash(state);
        self.authority.hash(state);
    }
}

impl Display for AuthoritySignInfo {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "AuthoritySignInfo {{ epoch: {:?}, authority: {} }}",
            self.epoch, self.authority,
        )
    }
}

impl PartialEq for AuthoritySignInfo {
    fn eq(&self, other: &Self) -> bool {
        // We do not compare the signature because there can be multiple
        // valid signatures for the same epoch and authority.
        self.epoch == other.epoch && self.authority == other.authority
    }
}

// ---------------------------------------------------------------------------
// Verus specifications
// ---------------------------------------------------------------------------

verus! {

// `AuthoritySignInfo` is defined outside `verus! { }` (the serde derives are
// not Verus-aware). Register it exactly as `ExAuthorityPublicKeyBytes` is
// registered — the type is owned by this crate so there is no orphan issue.
#[verifier::external_type_specification]
#[verifier::external_body]
pub struct ExAuthoritySignInfo(pub AuthoritySignInfo);

// Spec projectors connected to the getter methods above.
// We cannot access `self.epoch` directly (opaque datatype), so we attach
// specs to exec getters and expose them as spec functions.
pub uninterp spec fn auth_sig_epoch_spec(sig: &AuthoritySignInfo) -> u64;
pub uninterp spec fn auth_sig_authority_spec(sig: &AuthoritySignInfo) -> AuthorityName;

pub assume_specification[ AuthoritySignInfo::get_epoch ](sig: &AuthoritySignInfo) -> (e: u64)
    ensures e == auth_sig_epoch_spec(sig),
;

pub assume_specification[ AuthoritySignInfo::get_authority ](sig: &AuthoritySignInfo) -> (a: AuthorityName)
    ensures a == auth_sig_authority_spec(sig),
;

} // verus!
