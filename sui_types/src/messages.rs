// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::crypto::{sha3_hash, AuthoritySignature, BcsSignable, Signature};
use crate::object::{Object, ObjectFormatOptions, Owner, OBJECT_START_VERSION};

use super::{base_types::*, batch::*, committee::Committee, error::*, event::Event};

#[cfg(test)]
#[path = "unit_tests/messages_tests.rs"]
mod messages_tests;

use move_binary_format::{access::ModuleAccess, CompiledModule};
use move_core_types::{identifier::Identifier, language_storage::TypeTag, value::MoveStructLayout};
use serde::{Deserialize, Serialize};
use static_assertions::const_assert_eq;
use std::fmt::Write;
use std::fmt::{Display, Formatter};
use std::mem::size_of;
use std::{
    collections::{BTreeSet, HashSet},
    hash::{Hash, Hasher},
};
use strum::VariantNames;

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct Transfer {
    pub recipient: SuiAddress,
    pub object_ref: ObjectRef,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct MoveCall {
    // Although `package` represents a read-only Move package,
    // we still want to use a reference instead of just object ID.
    // This allows a client to be able to validate the package object
    // used in an order (through the object digest) without having to
    // re-execute the order on a quorum of authorities.
    pub package: ObjectRef,
    pub module: Identifier,
    pub function: Identifier,
    pub type_arguments: Vec<TypeTag>,
    pub object_arguments: Vec<ObjectRef>,
    pub shared_object_arguments: Vec<ObjectID>,
    pub pure_arguments: Vec<Vec<u8>>,
    pub gas_budget: u64,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct MoveModulePublish {
    pub modules: Vec<Vec<u8>>,
    pub gas_budget: u64,
}

#[derive(
    Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize, strum_macros::EnumVariantNames,
)]
pub enum TransactionKind {
    /// Initiate an object transfer between addresses
    Transfer(Transfer),
    /// Publish a new Move module
    Publish(MoveModulePublish),
    /// Call a function in a published Move module
    Call(MoveCall),
    // .. more transaction types go here
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct TransactionData {
    pub kind: TransactionKind,
    sender: SuiAddress,
    gas_payment: ObjectRef,
}

impl TransactionData {
    pub fn new(kind: TransactionKind, sender: SuiAddress, gas_payment: ObjectRef) -> Self {
        TransactionData {
            kind,
            sender,
            gas_payment,
        }
    }

    pub fn new_move_call(
        sender: SuiAddress,
        package: ObjectRef,
        module: Identifier,
        function: Identifier,
        type_arguments: Vec<TypeTag>,
        gas_payment: ObjectRef,
        object_arguments: Vec<ObjectRef>,
        shared_object_arguments: Vec<ObjectID>,
        pure_arguments: Vec<Vec<u8>>,
        gas_budget: u64,
    ) -> Self {
        let kind = TransactionKind::Call(MoveCall {
            package,
            module,
            function,
            type_arguments,
            object_arguments,
            shared_object_arguments,
            pure_arguments,
            gas_budget,
        });
        Self::new(kind, sender, gas_payment)
    }

    pub fn new_transfer(
        recipient: SuiAddress,
        object_ref: ObjectRef,
        sender: SuiAddress,
        gas_payment: ObjectRef,
    ) -> Self {
        let kind = TransactionKind::Transfer(Transfer {
            recipient,
            object_ref,
        });
        Self::new(kind, sender, gas_payment)
    }

    pub fn new_module(
        sender: SuiAddress,
        gas_payment: ObjectRef,
        modules: Vec<Vec<u8>>,
        gas_budget: u64,
    ) -> Self {
        let kind = TransactionKind::Publish(MoveModulePublish {
            modules,
            gas_budget,
        });
        Self::new(kind, sender, gas_payment)
    }

    /// Returns the transaction kind as a &str (variant name, no fields)
    pub fn kind_as_str(&self) -> &'static str {
        // NOTE: Ideally we could have used something like https://docs.rs/strum/latest/strum/derive.AsRefStr.html
        // The problem is that it doesn't actually return &'static ref due to &self above
        // and we really want 'static for common situations, such as authority_server dispatch where
        // by the time we instrument the transaction kind, the message or Transaction might have been moved
        // and so the lifetime and Kind is out of scope and we cannot borrow it.
        match self.kind {
            TransactionKind::Transfer(_) => TransactionKind::VARIANTS[0],
            TransactionKind::Publish(_) => TransactionKind::VARIANTS[1],
            TransactionKind::Call(_) => TransactionKind::VARIANTS[2],
        }
    }
}

/// An transaction signed by a client. signature is applied on data.
/// Any extension to Transaction should add fields to TransactionData, not Transaction.
// TODO: this should maybe be called ClientSignedTransaction + SignedTransaction -> AuthoritySignedTransaction
#[derive(Debug, Eq, Clone, Serialize, Deserialize)]
pub struct Transaction {
    pub data: TransactionData,
    pub signature: Signature,
}
const_assert_eq!(
    size_of::<TransactionData>() + size_of::<Signature>(),
    size_of::<Transaction>()
);

/// An transaction signed by a single authority
#[derive(Debug, Eq, Clone, Serialize, Deserialize)]
pub struct SignedTransaction {
    pub transaction: Transaction,
    pub authority: AuthorityName,
    pub signature: AuthoritySignature,
}

/// An transaction signed by a quorum of authorities
///
/// Note: the signature set of this data structure is not necessarily unique in the system,
/// i.e. there can be several valid certificates per transaction.
///
/// As a consequence, we check this struct does not implement Hash or Eq, see the note below.
///
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CertifiedTransaction {
    pub transaction: Transaction,
    pub signatures: Vec<(AuthorityName, AuthoritySignature)>,
}

// Note: if you meet an error due to this line it may be because you need an Eq implementation for `CertifiedTransaction`,
// or one of the structs that include it, i.e. `ConfirmationTransaction`, `TransactionInforResponse` or `ObjectInforResponse`.
//
// Please note that any such implementation must be agnostic to the exact set of signatures in the certificate, as
// clients are allowed to equivocate on the exact nature of valid certificates they send to the system. This assertion
// is a simple tool to make sure certifcates are accounted for correctly - should you remove it, you're on your own to
// maintain the invariant that valid certificates with distinct signatures are equivalent, but yet-unchecked
// certificates that differ on signers aren't.
//
// see also https://github.com/MystenLabs/sui/issues/266
//
static_assertions::assert_not_impl_any!(CertifiedTransaction: Hash, Eq, PartialEq);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConfirmationTransaction {
    pub certificate: CertifiedTransaction,
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct AccountInfoRequest {
    pub account: SuiAddress,
}

/// An information Request for batches, and their associated transactions
///
/// This reads historic data and sends the batch and transactions in the
/// database starting at the batch that includes `start`,
/// and then listens to new transactions until a batch equal or
/// is over the batch end marker.  
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct BatchInfoRequest {
    pub start: TxSequenceNumber,
    pub end: TxSequenceNumber,
}

#[derive(Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct BatchInfoResponseItem(pub UpdateItem);

impl From<SuiAddress> for AccountInfoRequest {
    fn from(account: SuiAddress) -> Self {
        AccountInfoRequest { account }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub enum ObjectInfoRequestKind {
    /// Request the latest object state, if a format option is provided,
    /// return the layout of the object in the given format.
    LatestObjectInfo(Option<ObjectFormatOptions>),
    /// Request the object state at a specific version
    PastObjectInfo(SequenceNumber),
}

/// A request for information about an object and optionally its
/// parent certificate at a specific version.
#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct ObjectInfoRequest {
    /// The id of the object to retrieve, at the latest version.
    pub object_id: ObjectID,
    /// The type of request, either latest object info or the past.
    pub request_kind: ObjectInfoRequestKind,
}

impl ObjectInfoRequest {
    pub fn past_object_info_request(object_id: ObjectID, version: SequenceNumber) -> Self {
        ObjectInfoRequest {
            object_id,
            request_kind: ObjectInfoRequestKind::PastObjectInfo(version),
        }
    }

    pub fn latest_object_info_request(
        object_id: ObjectID,
        layout: Option<ObjectFormatOptions>,
    ) -> Self {
        ObjectInfoRequest {
            object_id,
            request_kind: ObjectInfoRequestKind::LatestObjectInfo(layout),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Hash, Clone, Serialize, Deserialize)]
pub struct AccountInfoResponse {
    pub object_ids: Vec<ObjectRef>,
    pub owner: SuiAddress,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectResponse {
    /// Value of the requested object in this authority
    pub object: Object,
    /// Transaction the object is locked on in this authority.
    /// None if the object is not currently locked by this authority.
    pub lock: Option<SignedTransaction>,
    /// Schema of the Move value inside this object.
    /// None if the object is a Move package, or the request did not ask for the layout
    pub layout: Option<MoveStructLayout>,
}

/// This message provides information about the latest object and its lock
/// as well as the parent certificate of the object at a specific version.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ObjectInfoResponse {
    /// The certificate that created or mutated the object at a given version.
    /// If no parent certificate was requested the latest certificate concerning
    /// this object is sent. If the parent was requested and not found a error
    /// (ParentNotfound or CertificateNotfound) will be returned.
    pub parent_certificate: Option<CertifiedTransaction>,
    /// The full reference created by the above certificate
    pub requested_object_reference: Option<ObjectRef>,

    /// The object and its current lock, returned only if we are requesting
    /// the latest state of an object.
    /// If the object does not exist this is also None.
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
pub struct TransactionInfoRequest {
    pub transaction_digest: TransactionDigest,
}

impl From<TransactionDigest> for TransactionInfoRequest {
    fn from(transaction_digest: TransactionDigest) -> Self {
        TransactionInfoRequest { transaction_digest }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TransactionInfoResponse {
    // The signed transaction response to handle_transaction
    pub signed_transaction: Option<SignedTransaction>,
    // The certificate in case one is available
    pub certified_transaction: Option<CertifiedTransaction>,
    // The effects resulting from a successful execution should
    // contain ObjectRef created, mutated, deleted and events.
    pub signed_effects: Option<SignedTransactionEffects>,
}

#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub enum ExecutionStatus {
    // Gas used in the success case.
    Success { gas_used: u64 },
    // Gas used in the failed case, and the error.
    Failure { gas_used: u64, error: Box<SuiError> },
}

impl ExecutionStatus {
    pub fn new_failure(gas_used: u64, error: SuiError) -> ExecutionStatus {
        ExecutionStatus::Failure {
            gas_used,
            error: Box::new(error),
        }
    }

    pub fn is_ok(&self) -> bool {
        matches!(self, ExecutionStatus::Success { .. })
    }

    pub fn is_err(&self) -> bool {
        matches!(self, ExecutionStatus::Failure { .. })
    }

    pub fn unwrap(self) -> u64 {
        match self {
            ExecutionStatus::Success { gas_used } => gas_used,
            ExecutionStatus::Failure { .. } => {
                panic!("Unable to unwrap() on {:?}", self);
            }
        }
    }

    pub fn unwrap_err(self) -> (u64, SuiError) {
        match self {
            ExecutionStatus::Success { .. } => {
                panic!("Unable to unwrap() on {:?}", self);
            }
            ExecutionStatus::Failure { gas_used, error } => (gas_used, *error),
        }
    }

    /// Returns the gas used from the status
    pub fn gas_used(&self) -> u64 {
        match &self {
            ExecutionStatus::Success { gas_used } => *gas_used,
            ExecutionStatus::Failure { gas_used, .. } => *gas_used,
        }
    }
}

/// The response from processing a transaction or a certified transaction
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct TransactionEffects {
    // The status of the execution
    pub status: ExecutionStatus,
    // The transaction digest
    pub transaction_digest: TransactionDigest,
    // ObjectRef and owner of new objects created.
    pub created: Vec<(ObjectRef, Owner)>,
    // ObjectRef and owner of mutated objects, including gas object.
    pub mutated: Vec<(ObjectRef, Owner)>,
    // ObjectRef and owner of objects that are unwrapped in this transaction.
    // Unwrapped objects are objects that were wrapped into other objects in the past,
    // and just got extracted out.
    pub unwrapped: Vec<(ObjectRef, Owner)>,
    // Object Refs of objects now deleted (the old refs).
    pub deleted: Vec<ObjectRef>,
    // Object refs of objects now wrapped in other objects.
    pub wrapped: Vec<ObjectRef>,
    // The updated gas object reference. Have a dedicated field for convenient access.
    // It's also included in mutated.
    pub gas_object: (ObjectRef, Owner),
    /// The events emitted during execution. Note that only successful transactions emit events
    pub events: Vec<Event>,
    /// The set of transaction digests this transaction depends on.
    pub dependencies: Vec<TransactionDigest>,
}

impl TransactionEffects {
    /// Return an iterator that iterates through both mutated and
    /// created objects.
    /// It doesn't include deleted objects.
    pub fn mutated_and_created(&self) -> impl Iterator<Item = &(ObjectRef, Owner)> {
        self.mutated.iter().chain(self.created.iter())
    }

    /// Return an iterator of mutated objects, but excluding the gas object.
    pub fn mutated_excluding_gas(&self) -> impl Iterator<Item = &(ObjectRef, Owner)> {
        self.mutated.iter().filter(|o| *o != &self.gas_object)
    }

    pub fn is_object_mutated_here(&self, obj_ref: ObjectRef) -> bool {
        // The mutated or created case
        if self.mutated_and_created().any(|(oref, _)| *oref == obj_ref) {
            return true;
        }

        // The deleted case
        if obj_ref.2 == ObjectDigest::OBJECT_DIGEST_DELETED
            && self
                .deleted
                .iter()
                .any(|(id, seq, _)| *id == obj_ref.0 && seq.increment() == obj_ref.1)
        {
            return true;
        }

        // The wrapped case
        if obj_ref.2 == ObjectDigest::OBJECT_DIGEST_WRAPPED
            && self
                .wrapped
                .iter()
                .any(|(id, seq, _)| *id == obj_ref.0 && seq.increment() == obj_ref.1)
        {
            return true;
        }
        false
    }
}

impl BcsSignable for TransactionEffects {}

impl Display for TransactionEffects {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(writer, "Status : {:?}", self.status)?;
        if !self.created.is_empty() {
            writeln!(writer, "Created Objects:")?;
            for (obj, _) in &self.created {
                writeln!(writer, "{:?} {:?} {:?}", obj.0, obj.1, obj.2)?;
            }
        }
        if !self.mutated.is_empty() {
            writeln!(writer, "Mutated Objects:")?;
            for (obj, _) in &self.mutated {
                writeln!(writer, "{:?} {:?} {:?}", obj.0, obj.1, obj.2)?;
            }
        }
        if !self.deleted.is_empty() {
            writeln!(writer, "Deleted Objects:")?;
            for obj in &self.deleted {
                writeln!(writer, "{:?} {:?} {:?}", obj.0, obj.1, obj.2)?;
            }
        }
        write!(f, "{}", writer)
    }
}

/// An transaction signed by a single authority
#[derive(Eq, PartialEq, Clone, Debug, Serialize, Deserialize)]
pub struct SignedTransactionEffects {
    pub effects: TransactionEffects,
    pub authority: AuthorityName,
    pub signature: AuthoritySignature,
}

impl Hash for Transaction {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.data.hash(state);
    }
}

impl PartialEq for Transaction {
    fn eq(&self, other: &Self) -> bool {
        self.data == other.data
    }
}

impl Hash for SignedTransaction {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.transaction.hash(state);
        self.authority.hash(state);
    }
}

impl PartialEq for SignedTransaction {
    fn eq(&self, other: &Self) -> bool {
        self.transaction == other.transaction && self.authority == other.authority
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputObjectKind {
    MovePackage(ObjectID),
    OwnedMoveObject(ObjectRef),
    SharedMoveObject(ObjectID),
}

impl InputObjectKind {
    pub fn object_id(&self) -> ObjectID {
        match self {
            Self::MovePackage(id) => *id,
            Self::OwnedMoveObject((id, _, _)) => *id,
            Self::SharedMoveObject(id) => *id,
        }
    }

    pub fn version(&self) -> SequenceNumber {
        match self {
            Self::MovePackage(..) => OBJECT_START_VERSION,
            Self::OwnedMoveObject((_, version, _)) => *version,
            Self::SharedMoveObject(..) => OBJECT_START_VERSION,
        }
    }

    pub fn object_not_found_error(&self) -> SuiError {
        match *self {
            Self::MovePackage(package_id) => SuiError::DependentPackageNotFound { package_id },
            Self::OwnedMoveObject((object_id, _, _)) => SuiError::ObjectNotFound { object_id },
            Self::SharedMoveObject(object_id) => SuiError::ObjectNotFound { object_id },
        }
    }
}

impl Transaction {
    #[cfg(test)]
    pub fn from_data(data: TransactionData, signer: &dyn signature::Signer<Signature>) -> Self {
        let signature = Signature::new(&data, signer);
        Self::new(data, signature)
    }

    pub fn new(data: TransactionData, signature: Signature) -> Self {
        Transaction { data, signature }
    }

    pub fn check_signature(&self) -> Result<(), SuiError> {
        self.signature.check(&self.data, self.data.sender)
    }

    pub fn sender_address(&self) -> SuiAddress {
        self.data.sender
    }

    pub fn gas_payment_object_ref(&self) -> &ObjectRef {
        &self.data.gas_payment
    }

    pub fn contains_shared_object(&self) -> bool {
        match &self.data.kind {
            TransactionKind::Transfer(..) => false,
            TransactionKind::Call(c) => !c.shared_object_arguments.is_empty(),
            TransactionKind::Publish(..) => false,
        }
    }

    pub fn shared_input_objects(&self) -> &[ObjectID] {
        match &self.data.kind {
            TransactionKind::Call(c) => &c.shared_object_arguments,
            _ => &[],
        }
    }

    /// Return the metadata of each of the input objects for the transaction.
    /// For a Move object, we attach the object reference;
    /// for a Move package, we provide the object id only since they never change on chain.
    /// TODO: use an iterator over references here instead of a Vec to avoid allocations.
    pub fn input_objects(&self) -> Vec<InputObjectKind> {
        let mut inputs = match &self.data.kind {
            TransactionKind::Transfer(t) => {
                vec![InputObjectKind::OwnedMoveObject(t.object_ref)]
            }
            TransactionKind::Call(c) => {
                let mut call_inputs = Vec::with_capacity(2 + c.object_arguments.len());
                call_inputs.extend(
                    c.object_arguments
                        .clone()
                        .into_iter()
                        .map(InputObjectKind::OwnedMoveObject)
                        .collect::<Vec<_>>(),
                );
                call_inputs.extend(
                    c.shared_object_arguments
                        .iter()
                        .cloned()
                        .map(InputObjectKind::SharedMoveObject)
                        .collect::<Vec<_>>(),
                );
                call_inputs.push(InputObjectKind::MovePackage(c.package.0));
                call_inputs
            }
            TransactionKind::Publish(m) => {
                // For module publishing, all the dependent packages are implicit input objects
                // because they must all be on-chain in order for the package to publish.
                // All authorities must have the same view of those dependencies in order
                // to achieve consistent publish results.
                let compiled_modules = m
                    .modules
                    .iter()
                    .filter_map(|bytes| match CompiledModule::deserialize(bytes) {
                        Ok(m) => Some(m),
                        // We will ignore this error here and simply let latter execution
                        // to discover this error again and fail the transaction.
                        // It's preferrable to let transaction fail and charge gas when
                        // malformed package is provided.
                        Err(_) => None,
                    })
                    .collect::<Vec<_>>();
                Transaction::input_objects_in_compiled_modules(&compiled_modules)
            }
        };
        inputs.push(InputObjectKind::OwnedMoveObject(
            *self.gas_payment_object_ref(),
        ));
        inputs
    }

    // Derive a cryptographic hash of the transaction.
    pub fn digest(&self) -> TransactionDigest {
        TransactionDigest::new(sha3_hash(&self.data))
    }

    pub fn input_objects_in_compiled_modules(
        compiled_modules: &[CompiledModule],
    ) -> Vec<InputObjectKind> {
        let mut dependent_packages = BTreeSet::new();
        for module in compiled_modules.iter() {
            for handle in module.module_handles.iter() {
                let address = ObjectID::from(*module.address_identifier_at(handle.address));
                if address != ObjectID::ZERO {
                    dependent_packages.insert(address);
                }
            }
        }

        // We don't care about the digest of the dependent packages.
        // They are all read-only on-chain and their digest never changes.
        dependent_packages
            .into_iter()
            .map(InputObjectKind::MovePackage)
            .collect::<Vec<_>>()
    }
}

impl SignedTransaction {
    /// Use signing key to create a signed object.
    pub fn new(
        transaction: Transaction,
        authority: AuthorityName,
        secret: &dyn signature::Signer<AuthoritySignature>,
    ) -> Self {
        let signature = AuthoritySignature::new(&transaction.data, secret);
        Self {
            transaction,
            authority,
            signature,
        }
    }

    /// Verify the signature and return the non-zero voting right of the authority.
    pub fn check(&self, committee: &Committee) -> Result<usize, SuiError> {
        self.transaction.check_signature()?;
        let weight = committee.weight(&self.authority);
        fp_ensure!(weight > 0, SuiError::UnknownSigner);
        self.signature
            .check(&self.transaction.data, self.authority)?;
        Ok(weight)
    }
}

pub struct SignatureAggregator<'a> {
    committee: &'a Committee,
    weight: usize,
    used_authorities: HashSet<AuthorityName>,
    partial: CertifiedTransaction,
}

impl<'a> SignatureAggregator<'a> {
    /// Start aggregating signatures for the given value into a certificate.
    pub fn try_new(transaction: Transaction, committee: &'a Committee) -> Result<Self, SuiError> {
        transaction.check_signature()?;
        Ok(Self::new_unsafe(transaction, committee))
    }

    /// Same as try_new but we don't check the transaction.
    pub fn new_unsafe(transaction: Transaction, committee: &'a Committee) -> Self {
        Self {
            committee,
            weight: 0,
            used_authorities: HashSet::new(),
            partial: CertifiedTransaction {
                transaction,
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
        signature: AuthoritySignature,
    ) -> Result<Option<CertifiedTransaction>, SuiError> {
        signature.check(&self.partial.transaction.data, authority)?;
        // Check that each authority only appears once.
        fp_ensure!(
            !self.used_authorities.contains(&authority),
            SuiError::CertificateAuthorityReuse
        );
        self.used_authorities.insert(authority);
        // Update weight.
        let voting_rights = self.committee.weight(&authority);
        fp_ensure!(voting_rights > 0, SuiError::UnknownSigner);
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

impl CertifiedTransaction {
    /// Verify the certificate.
    pub fn check(&self, committee: &Committee) -> Result<(), SuiError> {
        // Check the quorum.
        let mut weight = 0;
        let mut used_authorities = HashSet::new();
        for (authority, _) in self.signatures.iter() {
            // Check that each authority only appears once.
            fp_ensure!(
                !used_authorities.contains(authority),
                SuiError::CertificateAuthorityReuse
            );
            used_authorities.insert(*authority);
            // Update weight.
            let voting_rights = committee.weight(authority);
            fp_ensure!(voting_rights > 0, SuiError::UnknownSigner);
            weight += voting_rights;
        }
        fp_ensure!(
            weight >= committee.quorum_threshold(),
            SuiError::CertificateRequiresQuorum
        );
        // All that is left is checking signatures!
        // one user signature
        self.transaction
            .signature
            .check(&self.transaction.data, self.transaction.data.sender)?;
        // a batch of authority signatures
        AuthoritySignature::verify_batch(
            &self.transaction.data,
            &self.signatures,
            &committee.expanded_keys,
        )
    }
}

impl Display for CertifiedTransaction {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let mut writer = String::new();
        writeln!(
            writer,
            "Signed Authorities : {:?}",
            self.signatures
                .iter()
                .map(|(name, _)| name)
                .collect::<Vec<_>>()
        )?;
        match &self.transaction.data.kind {
            TransactionKind::Transfer(t) => {
                writeln!(writer, "Transaction Kind : Transfer")?;
                writeln!(writer, "Recipient : {}", t.recipient)?;
                let (object_id, seq, digest) = t.object_ref;
                writeln!(writer, "Object ID : {}", &object_id)?;
                writeln!(writer, "Sequence Number : {:?}", seq)?;
                writeln!(writer, "Object Digest : {}", encode_bytes_hex(&digest.0))?;
            }
            TransactionKind::Publish(p) => {
                writeln!(writer, "Transaction Kind : Publish")?;
                writeln!(writer, "Gas Budget : {}", p.gas_budget)?;
            }
            TransactionKind::Call(c) => {
                writeln!(writer, "Transaction Kind : Call")?;
                writeln!(writer, "Gas Budget : {}", c.gas_budget)?;
                writeln!(writer, "Package ID : {}", c.package.0.to_hex_literal())?;
                writeln!(writer, "Module : {}", c.module)?;
                writeln!(writer, "Function : {}", c.function)?;
                writeln!(writer, "Object Arguments : {:?}", c.object_arguments)?;
                writeln!(writer, "Pure Arguments : {:?}", c.pure_arguments)?;
                writeln!(writer, "Type Arguments : {:?}", c.type_arguments)?;
            }
        }
        write!(f, "{}", writer)
    }
}

impl ConfirmationTransaction {
    pub fn new(certificate: CertifiedTransaction) -> Self {
        Self { certificate }
    }
}

impl BcsSignable for TransactionData {}
