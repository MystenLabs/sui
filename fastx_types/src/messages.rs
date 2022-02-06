// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::object::{Object, OBJECT_START_VERSION};

use super::{base_types::*, committee::Committee, error::*, event::Event};

#[cfg(test)]
#[path = "unit_tests/messages_tests.rs"]
mod messages_tests;

use move_binary_format::{access::ModuleAccess, CompiledModule};
use move_core_types::{identifier::Identifier, language_storage::TypeTag};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeSet, HashSet},
    hash::{Hash, Hasher},
};

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub sender: FastPayAddress,
    pub recipient: FastPayAddress,
    pub object_ref: ObjectRef,
    pub gas_payment: ObjectRef,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct MoveCall {
    pub sender: FastPayAddress,
    // TODO: For package object, we only need object id, as it's always read-only.
    pub package: ObjectRef,
    pub module: Identifier,
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
///
/// Note: the signature set of this data structure is not necessarily unique in the system,
/// i.e. there can be several valid certificates per transaction.
///
/// As a consequence, we check this struct does not implement Hash or Eq, see the note below.
///
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertifiedOrder {
    pub order: Order,
    pub signatures: Vec<(AuthorityName, Signature)>,
}

// Note: if you meet an error due to this line it may be because you need an Eq implementation for `CertifiedOrder`,
// or one of the structs that include it, i.e. `ConfirmationOrder`, `OrderInforResponse` or `ObjectInforResponse`.
//
// Please note that any such implementation must be agnostic to the exact set of signatures in the certificate, as
// clients are allowed to equivocate on the exact nature of valid certificates they send to the system. This assertion
// is a simple tool to make sure certifcates are accounted for correctly - should you remove it, you're on your own to
// maintain the invariant that valid certificates with distinct signatures are equivalent, but yet-unchecked
// certificates that differ on signers aren't.
//
// see also https://github.com/MystenLabs/fastnft/issues/266
//
static_assertions::assert_not_impl_any!(CertifiedOrder: Hash, Eq, PartialEq);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfirmationOrder {
    pub certificate: CertifiedOrder,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct AccountInfoRequest {
    pub account: FastPayAddress,
}

impl From<FastPayAddress> for AccountInfoRequest {
    fn from(account: FastPayAddress) -> Self {
        AccountInfoRequest { account }
    }
}

/// A request for information about an object and optionally its
/// parent certificate at a specific version.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ObjectInfoRequest {
    /// The id of the object to retrieve, at the latest version.
    pub object_id: ObjectID,
    /// The version of the object for which the parent certificate is sought.
    pub request_sequence_number: Option<SequenceNumber>,
}

impl From<ObjectRef> for ObjectInfoRequest {
    fn from(object_ref: ObjectRef) -> Self {
        ObjectInfoRequest {
            object_id: object_ref.0,
            request_sequence_number: Some(object_ref.1),
        }
    }
}

impl From<ObjectID> for ObjectInfoRequest {
    fn from(object_id: ObjectID) -> Self {
        ObjectInfoRequest {
            object_id,
            request_sequence_number: None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct AccountInfoResponse {
    pub object_ids: Vec<ObjectRef>,
    pub owner: FastPayAddress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectResponse {
    /// Value of the requested object in this authority
    pub object: Object,
    /// Order the object is locked on in this authority.
    /// None if the object is not currently locked by this authority.
    pub lock: Option<SignedOrder>,
}

/// This message provides information about the latest object and its lock
/// as well as the parent certificate of the object at a specific version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfoResponse {
    /// The certificate that created or mutated the object at a given version.
    /// If no parent certificate was requested the latest certificate concerning
    /// this object is sent. If the parent was requested and not found a error
    /// (ParentNotfound or CertificateNotfound) will be returned.
    pub parent_certificate: Option<CertifiedOrder>,
    /// The full reference created by the above certificate
    pub requested_object_reference: Option<ObjectRef>,

    /// The object and its current lock. If the object does not exist
    /// this is None.
    pub object_and_lock: Option<ObjectResponse>,
}

impl ObjectInfoResponse {
    pub fn object(&self) -> Option<&Object> {
        match &self.object_and_lock {
            Some(ObjectResponse { object, .. }) => Some(object),
            _ => None,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct OrderInfoRequest {
    pub transaction_digest: TransactionDigest,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrderInfoResponse {
    // The signed order response to handle_order
    pub signed_order: Option<SignedOrder>,
    // The certificate in case one is available
    pub certified_order: Option<CertifiedOrder>,
    // The effects resulting from a successful execution should
    // contain ObjectRef created, mutated, deleted and events.
    pub signed_effects: Option<SignedOrderEffects>,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Success,
    // Gas used in the failed case, and the error.
    // TODO: Eventually we should return gas_used in both cases.
    Failure {
        gas_used: u64,
        error: Box<FastPayError>,
    },
}

impl ExecutionStatus {
    pub fn unwrap(self) {
        match self {
            ExecutionStatus::Success => (),
            ExecutionStatus::Failure { .. } => {
                panic!("Unable to unwrap() on {:?}", self);
            }
        }
    }

    pub fn unwrap_err(self) -> (u64, FastPayError) {
        match self {
            ExecutionStatus::Success => {
                panic!("Unable to unwrap() on {:?}", self);
            }
            ExecutionStatus::Failure { gas_used, error } => (gas_used, *error),
        }
    }
}

/// The response from processing an order or a certified order
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct OrderEffects {
    // The status of the execution
    pub status: ExecutionStatus,
    // The transaction digest
    pub transaction_digest: TransactionDigest,
    // ObjectRef and owner of new objects created.
    pub created: Vec<(ObjectRef, Authenticator)>,
    // ObjectRef and owner of mutated objects.
    // mutated does not include gas object or created objects.
    pub mutated: Vec<(ObjectRef, Authenticator)>,
    // Object Refs of objects now deleted (the old refs).
    pub deleted: Vec<ObjectRef>,
    // The updated gas object reference.
    pub gas_object: (ObjectRef, Authenticator),
    /// The events emitted during execution. Note that only successful transactions emit events
    pub events: Vec<Event>,
    /// The set of transaction digests this order depends on.
    pub dependencies: Vec<TransactionDigest>,
}

impl OrderEffects {
    /// Return an iterator that iterates throguh all mutated objects,
    /// including all from mutated, created and the gas_object.
    /// It doesn't include deleted.
    pub fn all_mutated(&self) -> impl Iterator<Item = &(ObjectRef, Authenticator)> {
        self.mutated
            .iter()
            .chain(self.created.iter())
            .chain(std::iter::once(&self.gas_object))
    }
}

impl BcsSignable for OrderEffects {}

/// An order signed by a single authority
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct SignedOrderEffects {
    pub effects: OrderEffects,
    pub authority: AuthorityName,
    pub signature: Signature,
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputObjectKind {
    MovePackage(ObjectID),
    MoveObject(ObjectRef),
}

impl InputObjectKind {
    pub fn object_id(&self) -> ObjectID {
        match self {
            Self::MovePackage(id) => *id,
            Self::MoveObject((id, _, _)) => *id,
        }
    }

    pub fn version(&self) -> SequenceNumber {
        match self {
            Self::MovePackage(_) => OBJECT_START_VERSION,
            Self::MoveObject((_, version, _)) => *version,
        }
    }

    pub fn object_not_found_error(&self) -> FastPayError {
        match *self {
            Self::MovePackage(package_id) => FastPayError::DependentPackageNotFound { package_id },
            Self::MoveObject((object_id, _, _)) => FastPayError::ObjectNotFound { object_id },
        }
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
        package: ObjectRef,
        module: Identifier,
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
            package,
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

    /// Return the metadata of each of the input objects for the order.
    /// For a Move object, we attach the object reference;
    /// for a Move package, we provide the object id only since they never change on chain.
    /// TODO: use an iterator over references here instead of a Vec to avoid allocations.
    pub fn input_objects(&self) -> Vec<InputObjectKind> {
        match &self.kind {
            OrderKind::Transfer(t) => {
                vec![
                    InputObjectKind::MoveObject(t.object_ref),
                    InputObjectKind::MoveObject(t.gas_payment),
                ]
            }
            OrderKind::Call(c) => {
                let mut call_inputs = Vec::with_capacity(2 + c.object_arguments.len());
                call_inputs.extend(
                    c.object_arguments
                        .clone()
                        .into_iter()
                        .map(InputObjectKind::MoveObject)
                        .collect::<Vec<_>>(),
                );
                call_inputs.push(InputObjectKind::MovePackage(c.package.0));
                call_inputs.push(InputObjectKind::MoveObject(c.gas_payment));
                call_inputs
            }
            OrderKind::Publish(m) => {
                // For module publishing, all the dependent packages are implicit input objects
                // because they must all be on-chain in order for the package to publish.
                // All authorities must have the same view of those dependencies in order
                // to achieve consistent publish results.
                let mut dependent_packages = BTreeSet::new();
                for bytes in m.modules.iter() {
                    let module = match CompiledModule::deserialize(bytes) {
                        Ok(m) => m,
                        Err(_) => {
                            // We will ignore this error here and simply let latter execution
                            // to discover this error again and fail the transaction.
                            // It's preferrable to let transaction fail and charge gas when
                            // malformed package is provided.
                            continue;
                        }
                    };
                    for handle in module.module_handles.iter() {
                        let address = *module.address_identifier_at(handle.address);
                        if address != ObjectID::ZERO {
                            dependent_packages.insert(address);
                        }
                    }
                }
                // We don't care about the digest of the dependent packages.
                // They are all read-only on-chain and their digest never changes.
                let mut publish_inputs = dependent_packages
                    .into_iter()
                    .map(InputObjectKind::MovePackage)
                    .collect::<Vec<_>>();
                publish_inputs.push(InputObjectKind::MoveObject(m.gas_payment));
                publish_inputs
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

impl ConfirmationOrder {
    pub fn new(certificate: CertifiedOrder) -> Self {
        Self { certificate }
    }
}

impl BcsSignable for OrderKind {}
