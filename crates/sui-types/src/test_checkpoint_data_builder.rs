// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet, HashMap};

use move_core_types::{
    ident_str,
    language_storage::{StructTag, TypeTag},
};
use sui_protocol_config::ProtocolConfig;
use tap::Pipe;

use crate::{
    base_types::{
        dbg_addr, random_object_ref, ExecutionDigests, ObjectID, ObjectRef, SequenceNumber,
        SuiAddress,
    },
    committee::Committee,
    digests::TransactionDigest,
    effects::{TestEffectsBuilder, TransactionEffectsAPI, TransactionEvents},
    event::{Event, SystemEpochInfoEvent},
    full_checkpoint_content::{CheckpointData, CheckpointTransaction},
    gas_coin::GAS,
    message_envelope::Message,
    messages_checkpoint::{
        CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary, EndOfEpochData,
    },
    object::{MoveObject, Object, Owner, GAS_VALUE_FOR_TESTING},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        EndOfEpochTransactionKind, SenderSignedData, Transaction, TransactionData, TransactionKind,
    },
    SUI_SYSTEM_ADDRESS,
};

/// A builder for creating test checkpoint data.
/// Once initialized, the builder can be used to build multiple checkpoints.
/// Call `start_transaction` to begin creating a new transaction.
/// Call `finish_transaction` to complete the current transaction and add it to the current checkpoint.
/// After all transactions are added, call `build_checkpoint` to get the final checkpoint data.
/// This will also increment the stored checkpoint sequence number.
/// Start the above process again to build the next checkpoint.
/// NOTE: The generated checkpoint data is not guaranteed to be semantically valid or consistent.
/// For instance, all object digests will be randomly set. It focuses on providing a way to generate
/// various shaped test data for testing purposes.
/// If you need to test the validity of the checkpoint data, you should use Simulacrum instead.
pub struct TestCheckpointDataBuilder {
    /// Map of all live objects in the state.
    live_objects: HashMap<ObjectID, Object>,
    /// Map of all wrapped objects in the state.
    wrapped_objects: HashMap<ObjectID, Object>,
    /// A map from sender addresses to gas objects they own.
    /// These are created automatically when a transaction is started.
    /// Users of this builder should not need to worry about them.
    gas_map: HashMap<SuiAddress, ObjectID>,

    /// The current checkpoint builder.
    /// It is initialized when the builder is created, and is reset when `build_checkpoint` is called.
    checkpoint_builder: CheckpointBuilder,
}

struct CheckpointBuilder {
    /// Checkpoint number for the current checkpoint we are building.
    checkpoint: u64,
    /// Epoch number for the current checkpoint we are building.
    epoch: u64,
    /// Counter for the total number of transactions added to the builder.
    network_total_transactions: u64,
    /// Transactions that have been added to the current checkpoint.
    transactions: Vec<CheckpointTransaction>,
    /// The current transaction being built.
    next_transaction: Option<TransactionBuilder>,
}

struct TransactionBuilder {
    sender_idx: u8,
    gas: ObjectRef,
    move_calls: Vec<(ObjectID, &'static str, &'static str)>,
    created_objects: BTreeMap<ObjectID, Object>,
    mutated_objects: BTreeMap<ObjectID, Object>,
    unwrapped_objects: BTreeSet<ObjectID>,
    wrapped_objects: BTreeSet<ObjectID>,
    deleted_objects: BTreeSet<ObjectID>,
    events: Option<Vec<Event>>,
}

impl TransactionBuilder {
    pub fn new(sender_idx: u8, gas: ObjectRef) -> Self {
        Self {
            sender_idx,
            gas,
            move_calls: vec![],
            created_objects: BTreeMap::new(),
            mutated_objects: BTreeMap::new(),
            unwrapped_objects: BTreeSet::new(),
            wrapped_objects: BTreeSet::new(),
            deleted_objects: BTreeSet::new(),
            events: None,
        }
    }
}

impl TestCheckpointDataBuilder {
    pub fn new(checkpoint: u64) -> Self {
        Self {
            live_objects: HashMap::new(),
            wrapped_objects: HashMap::new(),
            gas_map: HashMap::new(),
            checkpoint_builder: CheckpointBuilder {
                checkpoint,
                epoch: 0,
                network_total_transactions: 0,
                transactions: vec![],
                next_transaction: None,
            },
        }
    }

    /// Set the epoch for the checkpoint.
    pub fn with_epoch(mut self, epoch: u64) -> Self {
        self.checkpoint_builder.epoch = epoch;
        self
    }

    /// Start creating a new transaction.
    /// `sender_idx` is a convenient representation of the sender's address.
    /// A proper SuiAddress will be derived from it.
    /// It will also create a gas object for the sender if it doesn't already exist in the live object map.
    /// You do not need to create the gas object yourself.
    pub fn start_transaction(mut self, sender_idx: u8) -> Self {
        assert!(self.checkpoint_builder.next_transaction.is_none());
        let sender = Self::derive_address(sender_idx);
        let gas_id = self.gas_map.entry(sender).or_insert_with(|| {
            let gas = Object::with_owner_for_testing(sender);
            let id = gas.id();
            self.live_objects.insert(id, gas);
            id
        });
        let gas_ref = self
            .live_objects
            .get(gas_id)
            .cloned()
            .unwrap()
            .compute_object_reference();
        self.checkpoint_builder.next_transaction =
            Some(TransactionBuilder::new(sender_idx, gas_ref));
        self
    }

    /// Create a new object in the transaction.
    /// `object_idx` is a convenient representation of the object's ID.
    /// The object will be created as a SUI coin object, with default balance,
    /// and the transaction sender as its owner.
    pub fn create_owned_object(self, object_idx: u64) -> Self {
        self.create_sui_object(object_idx, GAS_VALUE_FOR_TESTING)
    }

    /// Create a new shared object in the transaction.
    /// `object_idx` is a convenient representation of the object's ID.
    /// The object will be created as a SUI coin object, with default balance,
    /// and it is a shared object.
    pub fn create_shared_object(self, object_idx: u64) -> Self {
        self.create_coin_object_with_owner(
            object_idx,
            Owner::Shared {
                initial_shared_version: SequenceNumber::MIN,
            },
            GAS_VALUE_FOR_TESTING,
            GAS::type_tag(),
        )
    }

    /// Create a new SUI coin object in the transaction.
    /// `object_idx` is a convenient representation of the object's ID.
    /// `balance` is the amount of SUI to be created.
    pub fn create_sui_object(self, object_idx: u64, balance: u64) -> Self {
        let sender_idx = self
            .checkpoint_builder
            .next_transaction
            .as_ref()
            .unwrap()
            .sender_idx;
        self.create_coin_object(object_idx, sender_idx, balance, GAS::type_tag())
    }

    /// Create a new coin object in the transaction.
    /// `object_idx` is a convenient representation of the object's ID.
    /// `owner_idx` is a convenient representation of the object's owner's address.
    /// `balance` is the amount of SUI to be created.
    /// `coin_type` is the type of the coin to be created.
    pub fn create_coin_object(
        self,
        object_idx: u64,
        owner_idx: u8,
        balance: u64,
        coin_type: TypeTag,
    ) -> Self {
        self.create_coin_object_with_owner(
            object_idx,
            Owner::AddressOwner(Self::derive_address(owner_idx)),
            balance,
            coin_type,
        )
    }

    fn create_coin_object_with_owner(
        mut self,
        object_idx: u64,
        owner: Owner,
        balance: u64,
        coin_type: TypeTag,
    ) -> Self {
        let tx_builder = self.checkpoint_builder.next_transaction.as_mut().unwrap();
        let object_id = Self::derive_object_id(object_idx);
        assert!(
            !self.live_objects.contains_key(&object_id),
            "Object already exists: {}. Please use a different object index.",
            object_id
        );
        let move_object = MoveObject::new_coin(
            coin_type,
            // version doesn't matter since we will set it to the lamport version when we finalize the transaction
            SequenceNumber::MIN,
            object_id,
            balance,
        );
        let object = Object::new_move(move_object, owner, TransactionDigest::ZERO);
        tx_builder.created_objects.insert(object_id, object);
        self
    }

    /// Mutate an existing object in the transaction.
    /// `object_idx` is a convenient representation of the object's ID.
    pub fn mutate_object(mut self, object_idx: u64) -> Self {
        let tx_builder = self.checkpoint_builder.next_transaction.as_mut().unwrap();
        let object_id = Self::derive_object_id(object_idx);
        let object = self
            .live_objects
            .get(&object_id)
            .cloned()
            .expect("Mutating an object that doesn't exist");
        tx_builder.mutated_objects.insert(object_id, object);
        self
    }

    /// Transfer an existing object to a new owner.
    /// `object_idx` is a convenient representation of the object's ID.
    /// `recipient_idx` is a convenient representation of the recipient's address.
    pub fn transfer_object(self, object_idx: u64, recipient_idx: u8) -> Self {
        self.change_object_owner(
            object_idx,
            Owner::AddressOwner(Self::derive_address(recipient_idx)),
        )
    }

    /// Change the owner of an existing object.
    /// `object_idx` is a convenient representation of the object's ID.
    /// `owner` is the new owner of the object.
    pub fn change_object_owner(mut self, object_idx: u64, owner: Owner) -> Self {
        let tx_builder = self.checkpoint_builder.next_transaction.as_mut().unwrap();
        let object_id = Self::derive_object_id(object_idx);
        let mut object = self.live_objects.get(&object_id).unwrap().clone();
        object.owner = owner;
        tx_builder.mutated_objects.insert(object_id, object);
        self
    }

    /// Transfer part of an existing coin object's balance to a new owner.
    /// `object_idx` is a convenient representation of the object's ID.
    /// `new_object_idx` is a convenient representation of the new object's ID.
    /// `recipient_idx` is a convenient representation of the recipient's address.
    /// `amount` is the amount of balance to be transferred.
    pub fn transfer_coin_balance(
        mut self,
        object_idx: u64,
        new_object_idx: u64,
        recipient_idx: u8,
        amount: u64,
    ) -> Self {
        let tx_builder = self.checkpoint_builder.next_transaction.as_mut().unwrap();
        let object_id = Self::derive_object_id(object_idx);
        let mut object = self
            .live_objects
            .get(&object_id)
            .cloned()
            .expect("Mutating an object that does not exist");
        let coin_type = object.coin_type_maybe().unwrap();
        // Withdraw balance from coin object.
        let move_object = object.data.try_as_move_mut().unwrap();
        let old_balance = move_object.get_coin_value_unsafe();
        let new_balance = old_balance - amount;
        move_object.set_coin_value_unsafe(new_balance);
        tx_builder.mutated_objects.insert(object_id, object);

        // Deposit balance into new coin object.
        self.create_coin_object(new_object_idx, recipient_idx, amount, coin_type)
    }

    /// Wrap an existing object in the transaction.
    /// `object_idx` is a convenient representation of the object's ID.
    pub fn wrap_object(mut self, object_idx: u64) -> Self {
        let tx_builder = self.checkpoint_builder.next_transaction.as_mut().unwrap();
        let object_id = Self::derive_object_id(object_idx);
        assert!(self.live_objects.contains_key(&object_id));
        tx_builder.wrapped_objects.insert(object_id);
        self
    }

    /// Unwrap an existing object from the transaction.
    /// `object_idx` is a convenient representation of the object's ID.
    pub fn unwrap_object(mut self, object_idx: u64) -> Self {
        let tx_builder = self.checkpoint_builder.next_transaction.as_mut().unwrap();
        let object_id = Self::derive_object_id(object_idx);
        assert!(self.wrapped_objects.contains_key(&object_id));
        tx_builder.unwrapped_objects.insert(object_id);
        self
    }

    /// Delete an existing object from the transaction.
    /// `object_idx` is a convenient representation of the object's ID.
    pub fn delete_object(mut self, object_idx: u64) -> Self {
        let tx_builder = self.checkpoint_builder.next_transaction.as_mut().unwrap();
        let object_id = Self::derive_object_id(object_idx);
        assert!(self.live_objects.contains_key(&object_id));
        tx_builder.deleted_objects.insert(object_id);
        self
    }

    /// Add events to the transaction.
    /// `events` is a vector of events to be added to the transaction.
    pub fn with_events(mut self, events: Vec<Event>) -> Self {
        self.checkpoint_builder
            .next_transaction
            .as_mut()
            .unwrap()
            .events = Some(events);
        self
    }

    /// Add a move call PTB command to the transaction.
    /// `package` is the ID of the package to be called.
    /// `module` is the name of the module to be called.
    /// `function` is the name of the function to be called.
    pub fn add_move_call(
        mut self,
        package: ObjectID,
        module: &'static str,
        function: &'static str,
    ) -> Self {
        let tx_builder = self.checkpoint_builder.next_transaction.as_mut().unwrap();
        tx_builder.move_calls.push((package, module, function));
        self
    }

    /// Complete the current transaction and add it to the checkpoint. This will also finalize all
    /// the object changes, and reflect them in the live object map.
    pub fn finish_transaction(mut self) -> Self {
        let TransactionBuilder {
            sender_idx,
            gas,
            move_calls,
            created_objects,
            mutated_objects,
            unwrapped_objects,
            wrapped_objects,
            deleted_objects,
            events,
        } = self.checkpoint_builder.next_transaction.take().unwrap();
        let sender = Self::derive_address(sender_idx);
        let events = events.map(|events| TransactionEvents { data: events });
        let events_digest = events.as_ref().map(|events| events.digest());
        let mut pt_builder = ProgrammableTransactionBuilder::new();
        for (package, module, function) in move_calls {
            pt_builder
                .move_call(
                    package,
                    ident_str!(module).to_owned(),
                    ident_str!(function).to_owned(),
                    vec![],
                    vec![],
                )
                .unwrap();
        }
        let pt = pt_builder.finish();
        let tx_data = TransactionData::new(
            TransactionKind::ProgrammableTransaction(pt),
            sender,
            gas,
            1,
            1,
        );
        let tx = Transaction::new(SenderSignedData::new(tx_data, vec![]));
        let wrapped_objects: Vec<_> = wrapped_objects
            .into_iter()
            .map(|id| self.live_objects.remove(&id).unwrap())
            .collect();
        let deleted_objects: Vec<_> = deleted_objects
            .into_iter()
            .map(|id| self.live_objects.remove(&id).unwrap())
            .collect();
        let unwrapped_objects: Vec<_> = unwrapped_objects
            .into_iter()
            .map(|id| self.wrapped_objects.remove(&id).unwrap())
            .collect();
        let mut effects_builder = TestEffectsBuilder::new(tx.data())
            .with_created_objects(
                created_objects
                    .iter()
                    .map(|(id, o)| (*id, o.owner().clone())),
            )
            .with_mutated_objects(
                mutated_objects
                    .iter()
                    .map(|(id, o)| (*id, o.version(), o.owner().clone())),
            )
            .with_wrapped_objects(wrapped_objects.iter().map(|o| (o.id(), o.version())))
            .with_unwrapped_objects(
                unwrapped_objects
                    .iter()
                    .map(|o| (o.id(), o.owner().clone())),
            )
            .with_deleted_objects(deleted_objects.iter().map(|o| (o.id(), o.version())));
        if let Some(events_digest) = &events_digest {
            effects_builder = effects_builder.with_events_digest(*events_digest);
        }
        let effects = effects_builder.build();
        let lamport_version = effects.lamport_version();
        let input_objects: Vec<_> = mutated_objects
            .keys()
            .map(|id| self.live_objects.get(id).unwrap().clone())
            .chain(deleted_objects.clone())
            .chain(wrapped_objects.clone())
            .chain(std::iter::once(
                self.live_objects.get(&gas.0).unwrap().clone(),
            ))
            .collect();
        let output_objects: Vec<_> = created_objects
            .values()
            .cloned()
            .chain(mutated_objects.values().cloned())
            .chain(unwrapped_objects.clone())
            .chain(std::iter::once(
                self.live_objects.get(&gas.0).cloned().unwrap(),
            ))
            .map(|mut o| {
                o.data
                    .try_as_move_mut()
                    .unwrap()
                    .increment_version_to(lamport_version);
                o
            })
            .collect();
        self.live_objects
            .extend(output_objects.iter().map(|o| (o.id(), o.clone())));
        self.wrapped_objects
            .extend(wrapped_objects.iter().map(|o| (o.id(), o.clone())));
        self.checkpoint_builder
            .transactions
            .push(CheckpointTransaction {
                transaction: tx,
                effects,
                events,
                input_objects,
                output_objects,
            });
        self
    }

    /// Build the checkpoint data using all the transactions added to the builder so far.
    /// This will also increment the stored checkpoint sequence number.
    pub fn build_checkpoint(&mut self) -> CheckpointData {
        assert!(self.checkpoint_builder.next_transaction.is_none());
        let transactions = std::mem::take(&mut self.checkpoint_builder.transactions);
        let contents = CheckpointContents::new_with_digests_only_for_tests(
            transactions
                .iter()
                .map(|tx| ExecutionDigests::new(*tx.transaction.digest(), tx.effects.digest())),
        );
        self.checkpoint_builder.network_total_transactions += transactions.len() as u64;
        let checkpoint_summary = CheckpointSummary::new(
            &ProtocolConfig::get_for_max_version_UNSAFE(),
            self.checkpoint_builder.epoch,
            self.checkpoint_builder.checkpoint,
            self.checkpoint_builder.network_total_transactions,
            &contents,
            None,
            Default::default(),
            None,
            0,
            vec![],
        );
        let (committee, keys) = Committee::new_simple_test_committee();
        let checkpoint_cert = CertifiedCheckpointSummary::new_from_keypairs_for_testing(
            checkpoint_summary,
            &keys,
            &committee,
        );
        self.checkpoint_builder.checkpoint += 1;
        CheckpointData {
            checkpoint_summary: checkpoint_cert,
            checkpoint_contents: contents,
            transactions,
        }
    }

    /// Creates a transaction that advances the epoch, adds it to the checkpoint, and then builds
    /// the checkpoint. This increments the stored checkpoint sequence number and epoch. If
    /// `safe_mode` is true, the epoch end transaction will not include the `SystemEpochInfoEvent`.
    pub fn advance_epoch(&mut self, safe_mode: bool) -> CheckpointData {
        let (committee, _) = Committee::new_simple_test_committee();
        let protocol_config = ProtocolConfig::get_for_max_version_UNSAFE();
        let tx_kind = EndOfEpochTransactionKind::new_change_epoch(
            self.checkpoint_builder.epoch + 1,
            protocol_config.version,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );

        // TODO: need the system state object wrapper and dynamic field object to "correctly" mock
        // advancing epoch, at least to satisfy kv_epoch_starts pipeline.
        let end_of_epoch_tx = TransactionData::new(
            TransactionKind::EndOfEpochTransaction(vec![tx_kind]),
            SuiAddress::default(),
            random_object_ref(),
            1,
            1,
        )
        .pipe(|data| SenderSignedData::new(data, vec![]))
        .pipe(Transaction::new);

        let events = if !safe_mode {
            let system_epoch_info_event = SystemEpochInfoEvent {
                epoch: self.checkpoint_builder.epoch,
                protocol_version: protocol_config.version.as_u64(),
                ..Default::default()
            };
            let struct_tag = StructTag {
                address: SUI_SYSTEM_ADDRESS,
                module: ident_str!("sui_system_state_inner").to_owned(),
                name: ident_str!("SystemEpochInfoEvent").to_owned(),
                type_params: vec![],
            };
            Some(vec![Event::new(
                &SUI_SYSTEM_ADDRESS,
                ident_str!("sui_system_state_inner"),
                TestCheckpointDataBuilder::derive_address(0),
                struct_tag,
                bcs::to_bytes(&system_epoch_info_event).unwrap(),
            )])
        } else {
            None
        };

        let transaction_events = events.map(|events| TransactionEvents { data: events });

        // Similar to calling self.finish_transaction()
        self.checkpoint_builder
            .transactions
            .push(CheckpointTransaction {
                transaction: end_of_epoch_tx,
                effects: Default::default(),
                events: transaction_events,
                input_objects: vec![],
                output_objects: vec![],
            });

        // Call build_checkpoint() to finalize the checkpoint and then populate the checkpoint with
        // additional end of epoch data.
        let mut checkpoint = self.build_checkpoint();
        let end_of_epoch_data = EndOfEpochData {
            next_epoch_committee: committee.voting_rights.clone(),
            next_epoch_protocol_version: protocol_config.version,
            epoch_commitments: vec![],
        };
        checkpoint.checkpoint_summary.end_of_epoch_data = Some(end_of_epoch_data);
        self.checkpoint_builder.epoch += 1;
        checkpoint
    }

    /// Derive an object ID from an index. This is used to conveniently represent an object's ID.
    /// We ensure that the bytes of object IDs have a stable order that is the same as object_idx.
    pub fn derive_object_id(object_idx: u64) -> ObjectID {
        // We achieve this by setting the first 8 bytes of the object ID to the object_idx.
        let mut bytes = [0; ObjectID::LENGTH];
        bytes[0..8].copy_from_slice(&object_idx.to_le_bytes());
        ObjectID::from_bytes(bytes).unwrap()
    }

    /// Derive an address from an index.
    pub fn derive_address(address_idx: u8) -> SuiAddress {
        dbg_addr(address_idx)
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use move_core_types::ident_str;

    use crate::transaction::{Command, ProgrammableMoveCall, TransactionDataAPI};

    use super::*;
    #[test]
    fn test_basic_checkpoint_builder() {
        // Create a checkpoint with a single transaction that does nothing.
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .with_epoch(5)
            .start_transaction(0)
            .finish_transaction()
            .build_checkpoint();

        assert_eq!(*checkpoint.checkpoint_summary.sequence_number(), 1);
        assert_eq!(checkpoint.checkpoint_summary.epoch, 5);
        assert_eq!(checkpoint.transactions.len(), 1);
        let tx = &checkpoint.transactions[0];
        assert_eq!(
            tx.transaction.sender_address(),
            TestCheckpointDataBuilder::derive_address(0)
        );
        assert_eq!(tx.effects.mutated().len(), 1); // gas object
        assert_eq!(tx.effects.deleted().len(), 0);
        assert_eq!(tx.effects.created().len(), 0);
        assert_eq!(tx.input_objects.len(), 1);
        assert_eq!(tx.output_objects.len(), 1);
    }

    #[test]
    fn test_multiple_transactions() {
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .finish_transaction()
            .start_transaction(1)
            .finish_transaction()
            .start_transaction(2)
            .finish_transaction()
            .build_checkpoint();

        assert_eq!(checkpoint.transactions.len(), 3);

        // Verify transactions have different senders (since we used 0, 1, 2 as sender indices above).
        let senders: Vec<_> = checkpoint
            .transactions
            .iter()
            .map(|tx| tx.transaction.transaction_data().sender())
            .collect();
        assert_eq!(
            senders,
            vec![
                TestCheckpointDataBuilder::derive_address(0),
                TestCheckpointDataBuilder::derive_address(1),
                TestCheckpointDataBuilder::derive_address(2)
            ]
        );
    }

    #[test]
    fn test_object_creation() {
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .build_checkpoint();

        let tx = &checkpoint.transactions[0];
        let created_obj_id = TestCheckpointDataBuilder::derive_object_id(0);

        // Verify the newly created object appears in output objects
        assert!(tx
            .output_objects
            .iter()
            .any(|obj| obj.id() == created_obj_id));

        // Verify effects show object creation
        assert!(tx
            .effects
            .created()
            .iter()
            .any(|((id, ..), owner)| *id == created_obj_id
                && owner.get_owner_address().unwrap()
                    == TestCheckpointDataBuilder::derive_address(0)));
    }

    #[test]
    fn test_object_mutation() {
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .start_transaction(0)
            .mutate_object(0)
            .finish_transaction()
            .build_checkpoint();

        let tx = &checkpoint.transactions[1];
        let obj_id = TestCheckpointDataBuilder::derive_object_id(0);

        // Verify object appears in both input and output objects
        assert!(tx.input_objects.iter().any(|obj| obj.id() == obj_id));
        assert!(tx.output_objects.iter().any(|obj| obj.id() == obj_id));

        // Verify effects show object mutation
        assert!(tx
            .effects
            .mutated()
            .iter()
            .any(|((id, ..), _)| *id == obj_id));
    }

    #[test]
    fn test_object_deletion() {
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction()
            .build_checkpoint();

        let tx = &checkpoint.transactions[1];
        let obj_id = TestCheckpointDataBuilder::derive_object_id(0);

        // Verify object appears in input objects but not output
        assert!(tx.input_objects.iter().any(|obj| obj.id() == obj_id));
        assert!(!tx.output_objects.iter().any(|obj| obj.id() == obj_id));

        // Verify effects show object deletion
        assert!(tx.effects.deleted().iter().any(|(id, ..)| *id == obj_id));
    }

    #[test]
    fn test_object_wrapping() {
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .start_transaction(0)
            .wrap_object(0)
            .finish_transaction()
            .start_transaction(0)
            .unwrap_object(0)
            .finish_transaction()
            .build_checkpoint();

        let tx = &checkpoint.transactions[1];
        let obj_id = TestCheckpointDataBuilder::derive_object_id(0);

        // Verify object appears in input objects but not output
        assert!(tx.input_objects.iter().any(|obj| obj.id() == obj_id));
        assert!(!tx.output_objects.iter().any(|obj| obj.id() == obj_id));

        // Verify effects show object wrapping
        assert!(tx.effects.wrapped().iter().any(|(id, ..)| *id == obj_id));

        let tx = &checkpoint.transactions[2];

        // Verify object appears in output objects but not input
        assert!(!tx.input_objects.iter().any(|obj| obj.id() == obj_id));
        assert!(tx.output_objects.iter().any(|obj| obj.id() == obj_id));

        // Verify effects show object unwrapping
        assert!(tx
            .effects
            .unwrapped()
            .iter()
            .any(|((id, ..), _)| *id == obj_id));
    }

    #[test]
    fn test_object_transfer() {
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .start_transaction(1)
            .transfer_object(0, 1)
            .finish_transaction()
            .build_checkpoint();

        let tx = &checkpoint.transactions[1];
        let obj_id = TestCheckpointDataBuilder::derive_object_id(0);

        // Verify object appears in input and output objects
        assert!(tx.input_objects.iter().any(|obj| obj.id() == obj_id));
        assert!(tx.output_objects.iter().any(|obj| obj.id() == obj_id));

        // Verify effects show object transfer
        assert!(tx
            .effects
            .mutated()
            .iter()
            .any(|((id, ..), owner)| *id == obj_id
                && owner.get_owner_address().unwrap()
                    == TestCheckpointDataBuilder::derive_address(1)));
    }

    #[test]
    fn test_shared_object() {
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_shared_object(0)
            .finish_transaction()
            .build_checkpoint();

        let tx = &checkpoint.transactions[0];
        let obj_id = TestCheckpointDataBuilder::derive_object_id(0);

        // Verify object appears in output objects and is shared
        assert!(tx
            .output_objects
            .iter()
            .any(|obj| obj.id() == obj_id && obj.owner().is_shared()));
    }

    #[test]
    fn test_freeze_object() {
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .start_transaction(0)
            .change_object_owner(0, Owner::Immutable)
            .finish_transaction()
            .build_checkpoint();

        let tx = &checkpoint.transactions[1];
        let obj_id = TestCheckpointDataBuilder::derive_object_id(0);

        // Verify object appears in output objects and is immutable
        assert!(tx
            .output_objects
            .iter()
            .any(|obj| obj.id() == obj_id && obj.owner().is_immutable()));
    }

    #[test]
    fn test_sui_balance_transfer() {
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_sui_object(0, 100)
            .finish_transaction()
            .start_transaction(1)
            .transfer_coin_balance(0, 1, 1, 10)
            .finish_transaction()
            .build_checkpoint();

        let tx = &checkpoint.transactions[0];
        let obj_id0 = TestCheckpointDataBuilder::derive_object_id(0);

        // Verify the newly created object appears in output objects and is a gas coin with 100 MIST.
        assert!(tx.output_objects.iter().any(|obj| obj.id() == obj_id0
            && obj.is_gas_coin()
            && obj.data.try_as_move().unwrap().get_coin_value_unsafe() == 100));

        let tx = &checkpoint.transactions[1];
        let obj_id1 = TestCheckpointDataBuilder::derive_object_id(1);

        // Verify the original SUI coin now has 90 MIST after the transfer.
        assert!(tx.output_objects.iter().any(|obj| obj.id() == obj_id0
            && obj.is_gas_coin()
            && obj.data.try_as_move().unwrap().get_coin_value_unsafe() == 90));

        // Verify the split out SUI coin has 10 MIST.
        assert!(tx.output_objects.iter().any(|obj| obj.id() == obj_id1
            && obj.is_gas_coin()
            && obj.data.try_as_move().unwrap().get_coin_value_unsafe() == 10));
    }

    #[test]
    fn test_coin_balance_transfer() {
        let type_tag = TypeTag::from_str("0x100::a::b").unwrap();
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_coin_object(0, 0, 100, type_tag.clone())
            .finish_transaction()
            .start_transaction(1)
            .transfer_coin_balance(0, 1, 1, 10)
            .finish_transaction()
            .build_checkpoint();

        let tx = &checkpoint.transactions[1];
        let obj_id0 = TestCheckpointDataBuilder::derive_object_id(0);
        let obj_id1 = TestCheckpointDataBuilder::derive_object_id(1);

        // Verify the original coin now has 90 balance after the transfer.
        assert!(tx.output_objects.iter().any(|obj| obj.id() == obj_id0
            && obj.coin_type_maybe().unwrap() == type_tag
            && obj.data.try_as_move().unwrap().get_coin_value_unsafe() == 90));

        // Verify the split out coin has 10 balance, with the same type tag.
        assert!(tx.output_objects.iter().any(|obj| obj.id() == obj_id1
            && obj.coin_type_maybe().unwrap() == type_tag
            && obj.data.try_as_move().unwrap().get_coin_value_unsafe() == 10));
    }

    #[test]
    fn test_events() {
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .with_events(vec![Event::new(
                &ObjectID::ZERO,
                ident_str!("test"),
                TestCheckpointDataBuilder::derive_address(0),
                GAS::type_(),
                vec![],
            )])
            .finish_transaction()
            .build_checkpoint();
        let tx = &checkpoint.transactions[0];

        // Verify the transaction has an events digest
        assert!(tx.effects.events_digest().is_some());

        // Verify the transaction has a single event
        assert_eq!(tx.events.as_ref().unwrap().data.len(), 1);
    }

    #[test]
    fn test_move_call() {
        let checkpoint = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .add_move_call(ObjectID::ZERO, "test", "test")
            .finish_transaction()
            .build_checkpoint();
        let tx = &checkpoint.transactions[0];

        // Verify the transaction has a move call matching the arguments provided.
        assert!(tx
            .transaction
            .transaction_data()
            .kind()
            .iter_commands()
            .any(|cmd| {
                cmd == &Command::MoveCall(Box::new(ProgrammableMoveCall {
                    package: ObjectID::ZERO,
                    module: "test".to_string(),
                    function: "test".to_string(),
                    type_arguments: vec![],
                    arguments: vec![],
                }))
            }));
    }

    #[test]
    fn test_multiple_checkpoints() {
        let mut builder = TestCheckpointDataBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .mutate_object(0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint3 = builder.build_checkpoint();

        // Verify the sequence numbers are consecutive.
        assert_eq!(checkpoint1.checkpoint_summary.sequence_number, 1);
        assert_eq!(checkpoint2.checkpoint_summary.sequence_number, 2);
        assert_eq!(checkpoint3.checkpoint_summary.sequence_number, 3);
    }
}
