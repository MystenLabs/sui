// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use super::{base_types::*, committee::Committee, error::*};

#[cfg(test)]
#[path = "unit_tests/messages_tests.rs"]
mod messages_tests;

use move_core_types::{identifier::Identifier, language_storage::TypeTag};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    hash::{Hash, Hasher},
};

#[derive(Debug, PartialEq, Eq, Hash, Copy, Clone, Serialize, Deserialize)]
pub enum Address {
    Primary(PrimaryAddress),
    FastPay(FastPayAddress),
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub sender: FastPayAddress,
    pub recipient: Address,
    pub object_ref: ObjectRef,
    pub gas_payment: ObjectRef,
    pub user_data: UserData,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct MoveCall {
    pub sender: FastPayAddress,
    pub module: ObjectRef,
    pub function: Identifier,
    pub type_arguments: Vec<TypeTag>,
    pub gas_payment: ObjectRef,
    pub object_arguments: Vec<ObjectRef>,
    pub pure_arguments: Vec<Vec<u8>>,
    pub gas_budget: u64,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct MoveModulePublish {
    pub sender: FastPayAddress,
    pub gas_payment: ObjectRef,
    pub modules: Vec<Vec<u8>>,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum OrderKind {
    /// Initiate an object transfer between addresses
    Transfer(Transfer),
    /// Publish a new Move module
    Publish(MoveModulePublish),
    /// Call a function in a published Move module
    Call(MoveCall),
    // .. more order types go here
}

/// An order signed by a client
// TODO: this should maybe be called ClientSignedOrder + SignedOrder -> AuthoritySignedOrder
#[derive(Debug, Eq, Clone, Serialize, Deserialize)]
pub struct Order {
    pub kind: OrderKind,
    pub signature: Signature,
}

/// An order signed by a single authority
#[derive(Debug, Eq, Clone, Serialize, Deserialize)]
pub struct SignedOrder {
    pub order: Order,
    pub authority: AuthorityName,
    pub signature: Signature,
}

/// An order signed by a quorum of authorities
#[derive(Eq, Clone, Debug, Serialize, Deserialize)]
pub struct CertifiedOrder {
    pub order: Order,
    pub signatures: Vec<(AuthorityName, Signature)>,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct ConfirmationOrder {
    pub certificate: CertifiedOrder,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct RedeemTransaction {
    pub certificate: CertifiedOrder,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct AccountInfoRequest {
    pub object_id: ObjectID,
    pub request_sequence_number: Option<SequenceNumber>,
    pub request_received_transfers_excluding_first_nth: Option<usize>,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct AccountInfoResponse {
    pub object_id: ObjectID,
    pub owner: FastPayAddress,
    pub next_sequence_number: SequenceNumber,
    pub pending_confirmation: Option<SignedOrder>,
    pub requested_certificate: Option<CertifiedOrder>,
    pub requested_received_transfers: Vec<CertifiedOrder>,
}

impl Hash for Order {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.kind.hash(state);
    }
}

impl PartialEq for Order {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Hash for SignedOrder {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.order.hash(state);
        self.authority.hash(state);
    }
}

impl PartialEq for SignedOrder {
    fn eq(&self, other: &Self) -> bool {
        self.order == other.order && self.authority == other.authority
    }
}

impl Hash for CertifiedOrder {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.order.hash(state);
        self.signatures.len().hash(state);
        for (name, _) in self.signatures.iter() {
            name.hash(state);
        }
    }
}

impl PartialEq for CertifiedOrder {
    fn eq(&self, other: &Self) -> bool {
        self.order == other.order
            && self.signatures.len() == other.signatures.len()
            && self
                .signatures
                .iter()
                .map(|(name, _)| name)
                .eq(other.signatures.iter().map(|(name, _)| name))
    }
}

impl Order {
    pub fn new(kind: OrderKind, secret: &KeyPair) -> Self {
        let signature = Signature::new(&kind, secret);
        Order { kind, signature }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new_move_call(
        sender: FastPayAddress,
        module: ObjectRef,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_payment: ObjectRef,
        object_arguments: Vec<ObjectRef>,
        pure_arguments: Vec<Vec<u8>>,
        gas_budget: u64,
        secret: &KeyPair,
    ) -> Self {
        let kind = OrderKind::Call(MoveCall {
            sender,
            module,
            function,
            type_arguments,
            gas_payment,
            object_arguments,
            pure_arguments,
            gas_budget,
        });
        Self::new(kind, secret)
    }

    pub fn new_module(
        sender: FastPayAddress,
        gas_payment: ObjectRef,
        modules: Vec<Vec<u8>>,
        secret: &KeyPair,
    ) -> Self {
        let kind = OrderKind::Publish(MoveModulePublish {
            sender,
            gas_payment,
            modules,
        });
        Self::new(kind, secret)
    }

    pub fn new_transfer(transfer: Transfer, secret: &KeyPair) -> Self {
        Self::new(OrderKind::Transfer(transfer), secret)
    }

    pub fn check_signature(&self) -> Result<(), FastPayError> {
        self.signature.check(&self.kind, *self.sender())
    }

    // TODO: support orders with multiple objects, each with their own sequence number (https://github.com/MystenLabs/fastnft/issues/8)
    pub fn sequence_number(&self) -> SequenceNumber {
        use OrderKind::*;
        match &self.kind {
            Transfer(t) => t.object_ref.1,
            Publish(_) => SequenceNumber::new(), // modules are immutable, seq # is always 0
            Call(c) => {
                assert!(
                    c.object_arguments.is_empty(),
                    "Unimplemented: non-gas object arguments"
                );
                c.gas_payment.1
            }
        }
    }

    /// Return the set of input objects for this order
    /// TODO: use an iterator over references here instead of a Vec to avoid allocations.
    pub fn input_objects(&self) -> Vec<ObjectRef> {
        match &self.kind {
            OrderKind::Transfer(t) => {
                vec![t.object_ref, t.gas_payment]
            }
            OrderKind::Call(c) => {
                let mut call_inputs = Vec::with_capacity(2 + c.object_arguments.len());
                call_inputs.extend(c.object_arguments.clone());
                call_inputs.push(c.module);
                call_inputs.push(c.gas_payment);
                call_inputs
            }
            OrderKind::Publish(m) => {
                vec![m.gas_payment]
            }
        }
    }

    // TODO: support orders with multiple objects (https://github.com/MystenLabs/fastnft/issues/8)
    pub fn object_id(&self) -> &ObjectID {
        use OrderKind::*;
        match &self.kind {
            Transfer(t) => &t.object_ref.0,
            Publish(m) => &m.gas_payment.0,
            Call(c) => {
                assert!(
                    c.object_arguments.is_empty(),
                    "Unimplemented: non-gas object arguments"
                );
                &c.gas_payment.0
            }
        }
    }

    pub fn gas_payment_object_id(&self) -> &ObjectID {
        use OrderKind::*;
        match &self.kind {
            Transfer(t) => &t.gas_payment.0,
            Publish(m) => &m.gas_payment.0,
            Call(c) => &c.gas_payment.0,
        }
    }

    // TODO: make sender a field of Order
    pub fn sender(&self) -> &FastPayAddress {
        use OrderKind::*;
        match &self.kind {
            Transfer(t) => &t.sender,
            Publish(m) => &m.sender,
            Call(c) => &c.sender,
        }
    }

    // Derive a cryptographic hash of the transaction.
    pub fn digest(&self) -> TransactionDigest {
        TransactionDigest::new(sha3_hash(&self.kind))
    }
}

impl SignedOrder {
    /// Use signing key to create a signed object.
    pub fn new(order: Order, authority: AuthorityName, secret: &KeyPair) -> Self {
        let signature = Signature::new(&order.kind, secret);
        Self {
            order,
            authority,
            signature,
        }
    }

    /// Verify the signature and return the non-zero voting right of the authority.
    pub fn check(&self, committee: &Committee) -> Result<usize, FastPayError> {
        self.order.check_signature()?;
        let weight = committee.weight(&self.authority);
        fp_ensure!(weight > 0, FastPayError::UnknownSigner);
        self.signature.check(&self.order.kind, self.authority)?;
        Ok(weight)
    }
}

pub struct SignatureAggregator<'a> {
    committee: &'a Committee,
    weight: usize,
    used_authorities: HashSet<AuthorityName>,
    partial: CertifiedOrder,
}

impl<'a> SignatureAggregator<'a> {
    /// Start aggregating signatures for the given value into a certificate.
    pub fn try_new(order: Order, committee: &'a Committee) -> Result<Self, FastPayError> {
        order.check_signature()?;
        Ok(Self::new_unsafe(order, committee))
    }

    /// Same as try_new but we don't check the order.
    pub fn new_unsafe(order: Order, committee: &'a Committee) -> Self {
        Self {
            committee,
            weight: 0,
            used_authorities: HashSet::new(),
            partial: CertifiedOrder {
                order,
                signatures: Vec::new(),
            },
        }
    }

    /// Try to append a signature to a (partial) certificate. Returns Some(certificate) if a quorum was reached.
    /// The resulting final certificate is guaranteed to be valid in the sense of `check` below.
    /// Returns an error if the signed value cannot be aggregated.
    pub fn append(
        &mut self,
        authority: AuthorityName,
        signature: Signature,
    ) -> Result<Option<CertifiedOrder>, FastPayError> {
        signature.check(&self.partial.order.kind, authority)?;
        // Check that each authority only appears once.
        fp_ensure!(
            !self.used_authorities.contains(&authority),
            FastPayError::CertificateAuthorityReuse
        );
        self.used_authorities.insert(authority);
        // Update weight.
        let voting_rights = self.committee.weight(&authority);
        fp_ensure!(voting_rights > 0, FastPayError::UnknownSigner);
        self.weight += voting_rights;
        // Update certificate.
        self.partial.signatures.push((authority, signature));

        if self.weight >= self.committee.quorum_threshold() {
            Ok(Some(self.partial.clone()))
        } else {
            Ok(None)
        }
    }
}

impl CertifiedOrder {
    /// Verify the certificate.
    pub fn check(&self, committee: &Committee) -> Result<(), FastPayError> {
        // Check the quorum.
        let mut weight = 0;
        let mut used_authorities = HashSet::new();
        for (authority, _) in self.signatures.iter() {
            // Check that each authority only appears once.
            fp_ensure!(
                !used_authorities.contains(authority),
                FastPayError::CertificateAuthorityReuse
            );
            used_authorities.insert(*authority);
            // Update weight.
            let voting_rights = committee.weight(authority);
            fp_ensure!(voting_rights > 0, FastPayError::UnknownSigner);
            weight += voting_rights;
        }
        fp_ensure!(
            weight >= committee.quorum_threshold(),
            FastPayError::CertificateRequiresQuorum
        );
        // All that is left is checking signatures!
        let inner_sig = (*self.order.sender(), self.order.signature);
        Signature::verify_batch(
            &self.order.kind,
            std::iter::once(&inner_sig).chain(&self.signatures),
            &committee.expanded_keys,
        )
    }
}

impl RedeemTransaction {
    pub fn new(certificate: CertifiedOrder) -> Self {
        Self { certificate }
    }
}

impl ConfirmationOrder {
    pub fn new(certificate: CertifiedOrder) -> Self {
        Self { certificate }
    }
}

impl BcsSignable for OrderKind {}
