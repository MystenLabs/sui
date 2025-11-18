// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_core_types::{
    ident_str,
    language_storage::{StructTag, TypeTag},
};
use std::collections::{BTreeMap, BTreeSet, HashMap};
use sui_protocol_config::{ProtocolConfig, ProtocolVersion};
use sui_sdk_types::CheckpointTimestamp;

use crate::messages_checkpoint::CheckpointCommitment;
use crate::{
    SUI_SYSTEM_ADDRESS,
    base_types::{
        ExecutionDigests, ObjectID, ObjectRef, SequenceNumber, SuiAddress, dbg_addr,
        random_object_ref,
    },
    committee::Committee,
    digests::TransactionDigest,
    effects::{
        self, TestEffectsBuilder, TransactionEffects, TransactionEffectsAPI, TransactionEvents,
    },
    event::{Event, SystemEpochInfoEvent},
    execution_status::ExecutionStatus,
    full_checkpoint_content::{Checkpoint, CheckpointTransaction, ExecutedTransaction, ObjectSet},
    gas::GasCostSummary,
    gas_coin::GAS,
    message_envelope::Message,
    messages_checkpoint::{
        CertifiedCheckpointSummary, CheckpointContents, CheckpointSummary, EndOfEpochData,
    },
    object::{GAS_VALUE_FOR_TESTING, MoveObject, Object, Owner},
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    transaction::{
        EndOfEpochTransactionKind, ObjectArg, SenderSignedData, SharedObjectMutability,
        Transaction, TransactionData, TransactionKind,
    },
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
pub struct TestCheckpointBuilder {
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
    /// Timestamp of the checkpoint.
    timestamp_ms: CheckpointTimestamp,
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
    frozen_objects: BTreeSet<ObjectRef>,
    shared_inputs: BTreeMap<ObjectID, Shared>,
    events: Option<Vec<Event>>,
}

struct Shared {
    mutable: bool,
    object: Object,
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
            frozen_objects: BTreeSet::new(),
            shared_inputs: BTreeMap::new(),
            events: None,
        }
    }
}

pub struct AdvanceEpochConfig {
    pub safe_mode: bool,
    pub protocol_version: ProtocolVersion,
    pub output_objects: Vec<Object>,
    pub epoch_commitments: Vec<CheckpointCommitment>,
}

impl Default for AdvanceEpochConfig {
    fn default() -> Self {
        Self {
            safe_mode: false,
            protocol_version: ProtocolVersion::MAX,
            output_objects: vec![],
            epoch_commitments: vec![],
        }
    }
}

impl TestCheckpointBuilder {
    pub fn new(checkpoint: u64) -> Self {
        Self {
            live_objects: HashMap::new(),
            wrapped_objects: HashMap::new(),
            gas_map: HashMap::new(),
            checkpoint_builder: CheckpointBuilder {
                checkpoint,
                epoch: 0,
                network_total_transactions: 0,
                timestamp_ms: 0,
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

    /// Set the network_total_transactions for the checkpoint.
    pub fn with_network_total_transactions(mut self, network_total_transactions: u64) -> Self {
        self.checkpoint_builder.network_total_transactions = network_total_transactions;
        self
    }

    /// Set the timestamp for the checkpoint.
    pub fn with_timestamp_ms(mut self, timestamp_ms: CheckpointTimestamp) -> Self {
        self.checkpoint_builder.timestamp_ms = timestamp_ms;
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

    /// Mutate an existing owned object in the transaction.
    /// `object_idx` is a convenient representation of the object's ID.
    pub fn mutate_owned_object(mut self, object_idx: u64) -> Self {
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

    /// Mutate an existing shared object in the transaction.
    pub fn mutate_shared_object(self, object_idx: u64) -> Self {
        self.access_shared_object(object_idx, true)
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

    /// Add an immutable object as an input to the transaction.
    ///
    /// Fails if the object is not live or if its owner is not [Owner::Immutable]).
    pub fn read_frozen_object(mut self, object_id: u64) -> Self {
        let tx_builder = self.checkpoint_builder.next_transaction.as_mut().unwrap();
        let object_id = Self::derive_object_id(object_id);

        let Some(obj) = self.live_objects.get(&object_id) else {
            panic!("Frozen object not found");
        };

        assert!(obj.owner().is_immutable());
        tx_builder
            .frozen_objects
            .insert(obj.compute_object_reference());
        self
    }

    /// Add a read to a shared object to the transaction's effects.
    pub fn read_shared_object(self, object_idx: u64) -> Self {
        self.access_shared_object(object_idx, false)
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
            frozen_objects,
            shared_inputs,
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

        for &object_ref in &frozen_objects {
            pt_builder
                .obj(ObjectArg::ImmOrOwnedObject(object_ref))
                .expect("Failed to add frozen object input");
        }

        for (id, input) in &shared_inputs {
            let &Owner::Shared {
                initial_shared_version,
            } = input.object.owner()
            else {
                panic!("Accessing a non-shared object as shared");
            };

            pt_builder
                .obj(ObjectArg::SharedObject {
                    id: *id,
                    initial_shared_version,
                    mutability: if input.mutable {
                        SharedObjectMutability::Mutable
                    } else {
                        SharedObjectMutability::Immutable
                    },
                })
                .expect("Failed to add shared object input");
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
            .with_deleted_objects(deleted_objects.iter().map(|o| (o.id(), o.version())))
            .with_frozen_objects(frozen_objects.into_iter().map(|(id, _, _)| id))
            .with_shared_input_versions(
                shared_inputs
                    .iter()
                    .map(|(id, input)| (*id, input.object.version()))
                    .collect(),
            );

        if let Some(events_digest) = &events_digest {
            effects_builder = effects_builder.with_events_digest(*events_digest);
        }

        let effects = effects_builder.build();
        let lamport_version = effects.lamport_version();
        let input_objects: Vec<_> = mutated_objects
            .keys()
            .chain(
                shared_inputs
                    .iter()
                    .filter(|(_, i)| i.mutable)
                    .map(|(id, _)| id),
            )
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
            .chain(
                shared_inputs
                    .values()
                    .filter(|i| i.mutable)
                    .map(|i| i.object.clone()),
            )
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
    pub fn build_checkpoint(&mut self) -> Checkpoint {
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
            self.checkpoint_builder.timestamp_ms,
            vec![],
            Vec::new(),
        );
        let (committee, keys) = Committee::new_simple_test_committee();
        let checkpoint_cert = CertifiedCheckpointSummary::new_from_keypairs_for_testing(
            checkpoint_summary,
            &keys,
            &committee,
        );
        self.checkpoint_builder.checkpoint += 1;

        // Build the object set and convert transactions to ExecutedTransaction
        let mut object_set = ObjectSet::default();
        let executed_transactions = transactions
            .into_iter()
            .map(|tx| {
                // Insert all input and output objects into the object set
                for o in tx.input_objects.into_iter().chain(tx.output_objects) {
                    object_set.insert(o);
                }

                // Extract TransactionData and signatures from Transaction
                let sender_signed = tx.transaction.into_data().into_inner();

                ExecutedTransaction {
                    transaction: sender_signed.intent_message.value,
                    signatures: sender_signed.tx_signatures,
                    effects: tx.effects,
                    events: tx.events,
                    unchanged_loaded_runtime_objects: Vec::new(),
                }
            })
            .collect();

        Checkpoint {
            summary: checkpoint_cert,
            contents,
            transactions: executed_transactions,
            object_set,
        }
    }

    /// Creates a transaction that advances the epoch, adds it to the checkpoint, and then builds
    /// the checkpoint. This increments the stored checkpoint sequence number and epoch. If
    /// `safe_mode` is true, the epoch end transaction will not include the `SystemEpochInfoEvent`.
    /// The `protocol_version` is used to set the protocol that we are going to follow in the
    /// subsequent epoch.
    pub fn advance_epoch(
        &mut self,
        AdvanceEpochConfig {
            safe_mode,
            protocol_version,
            output_objects,
            epoch_commitments,
        }: AdvanceEpochConfig,
    ) -> Checkpoint {
        let (committee, _) = Committee::new_simple_test_committee();
        let tx_kind = EndOfEpochTransactionKind::new_change_epoch(
            self.checkpoint_builder.epoch + 1,
            protocol_version,
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
            Default::default(),
        );
        let end_of_epoch_tx_data = TransactionData::new(
            TransactionKind::EndOfEpochTransaction(vec![tx_kind]),
            SuiAddress::default(),
            random_object_ref(),
            1,
            1,
        );
        let end_of_epoch_tx_signed = SenderSignedData::new(end_of_epoch_tx_data, vec![]);
        let end_of_epoch_tx = Transaction::new(end_of_epoch_tx_signed.clone());

        let events = if !safe_mode {
            let system_epoch_info_event = SystemEpochInfoEvent {
                epoch: self.checkpoint_builder.epoch,
                protocol_version: protocol_version.as_u64(),
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
                TestCheckpointBuilder::derive_address(0),
                struct_tag,
                bcs::to_bytes(&system_epoch_info_event).unwrap(),
            )])
        } else {
            None
        };

        let transaction_events = events.map(|events| TransactionEvents { data: events });
        let events_digest = transaction_events.as_ref().map(|events| events.digest());

        let changed_objects = output_objects
            .iter()
            .map(|obj| {
                (
                    obj.id(),
                    effects::EffectsObjectChange {
                        input_state: effects::ObjectIn::NotExist,
                        output_state: effects::ObjectOut::ObjectWrite((
                            obj.digest(),
                            obj.owner().clone(),
                        )),
                        id_operation: effects::IDOperation::Created,
                    },
                )
            })
            .collect();

        let lamport_version = SequenceNumber::from_u64(1);

        let output_objects: Vec<Object> = output_objects
            .into_iter()
            .map(|mut obj| {
                if let Some(move_obj) = obj.data.try_as_move_mut() {
                    move_obj.increment_version_to(lamport_version);
                }
                obj
            })
            .collect();

        let effects = TransactionEffects::new_from_execution_v2(
            ExecutionStatus::Success,
            self.checkpoint_builder.epoch,
            GasCostSummary::default(),
            vec![],
            BTreeSet::new(),
            end_of_epoch_tx_signed.digest(),
            lamport_version,
            changed_objects,
            None,
            events_digest,
            vec![],
        );
        self.checkpoint_builder
            .transactions
            .push(CheckpointTransaction {
                transaction: end_of_epoch_tx,
                effects,
                events: transaction_events,
                input_objects: vec![],
                output_objects,
            });
        let mut checkpoint = self.build_checkpoint();
        let end_of_epoch_data = EndOfEpochData {
            next_epoch_committee: committee.voting_rights.clone(),
            next_epoch_protocol_version: protocol_version,
            epoch_commitments,
        };
        checkpoint.summary.end_of_epoch_data = Some(end_of_epoch_data);
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

    /// Add a shared input to the transaction, being accessed from the currently recorded live
    /// version.
    fn access_shared_object(mut self, object_idx: u64, mutability: bool) -> Self {
        let tx_builder = self.checkpoint_builder.next_transaction.as_mut().unwrap();
        let object_id = Self::derive_object_id(object_idx);
        let object = self
            .live_objects
            .get(&object_id)
            .cloned()
            .expect("Accessing a shared object that doesn't exist");
        tx_builder.shared_inputs.insert(
            object_id,
            Shared {
                mutable: mutability,
                object,
            },
        );
        self
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
        let checkpoint = TestCheckpointBuilder::new(1)
            .with_epoch(5)
            .start_transaction(0)
            .finish_transaction()
            .build_checkpoint();

        assert_eq!(*checkpoint.summary.sequence_number(), 1);
        assert_eq!(checkpoint.summary.epoch, 5);
        assert_eq!(checkpoint.transactions.len(), 1);
        let tx = &checkpoint.transactions[0];
        assert_eq!(
            tx.transaction.sender(),
            TestCheckpointBuilder::derive_address(0)
        );
        assert_eq!(tx.effects.mutated().len(), 1); // gas object
        assert_eq!(tx.effects.deleted().len(), 0);
        assert_eq!(tx.effects.created().len(), 0);
        // object_set contains both input and output versions (2 total: input gas + output gas)
        assert_eq!(checkpoint.object_set.iter().count(), 2);
    }

    #[test]
    fn test_multiple_transactions() {
        let checkpoint = TestCheckpointBuilder::new(1)
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
            .map(|tx| tx.transaction.sender())
            .collect();
        assert_eq!(
            senders,
            vec![
                TestCheckpointBuilder::derive_address(0),
                TestCheckpointBuilder::derive_address(1),
                TestCheckpointBuilder::derive_address(2)
            ]
        );
    }

    #[test]
    fn test_object_creation() {
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .build_checkpoint();

        let tx = &checkpoint.transactions[0];
        let created_obj_id = TestCheckpointBuilder::derive_object_id(0);

        // Verify the newly created object appears in the object set
        assert!(
            checkpoint
                .object_set
                .iter()
                .any(|obj| obj.id() == created_obj_id)
        );

        // Verify effects show object creation
        assert!(
            tx.effects
                .created()
                .iter()
                .any(|((id, ..), owner)| *id == created_obj_id
                    && owner.get_owner_address().unwrap()
                        == TestCheckpointBuilder::derive_address(0))
        );
    }

    #[test]
    fn test_object_mutation() {
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .start_transaction(0)
            .mutate_owned_object(0)
            .finish_transaction()
            .build_checkpoint();

        let obj_id = TestCheckpointBuilder::derive_object_id(0);

        // Verify object is in the object set
        assert!(checkpoint.object_set.iter().any(|obj| obj.id() == obj_id));

        // Verify effects show object mutation
        let tx = &checkpoint.transactions[1];
        assert!(
            tx.effects
                .mutated()
                .iter()
                .any(|((id, ..), _)| *id == obj_id)
        );
    }

    #[test]
    fn test_object_deletion() {
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction()
            .build_checkpoint();

        let obj_id = TestCheckpointBuilder::derive_object_id(0);

        // The deleted object is still in object_set (it contains both inputs and outputs)
        // We verify deletion via the effects instead
        assert!(checkpoint.object_set.iter().any(|obj| obj.id() == obj_id));

        // Verify effects show object deletion
        let tx = &checkpoint.transactions[1];
        assert!(tx.effects.deleted().iter().any(|(id, ..)| *id == obj_id));
    }

    #[test]
    fn test_object_wrapping() {
        let checkpoint = TestCheckpointBuilder::new(1)
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

        let obj_id = TestCheckpointBuilder::derive_object_id(0);

        // After wrap and unwrap, object should be in the final object set
        assert!(checkpoint.object_set.iter().any(|obj| obj.id() == obj_id));

        // Verify effects show object wrapping
        let tx = &checkpoint.transactions[1];
        assert!(tx.effects.wrapped().iter().any(|(id, ..)| *id == obj_id));

        // Verify effects show object unwrapping
        let tx = &checkpoint.transactions[2];
        assert!(
            tx.effects
                .unwrapped()
                .iter()
                .any(|((id, ..), _)| *id == obj_id)
        );
    }

    #[test]
    fn test_object_transfer() {
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .start_transaction(1)
            .transfer_object(0, 1)
            .finish_transaction()
            .build_checkpoint();

        let obj_id = TestCheckpointBuilder::derive_object_id(0);

        // Object should be in the final object set
        assert!(checkpoint.object_set.iter().any(|obj| obj.id() == obj_id));

        // Verify effects show object transfer
        let tx = &checkpoint.transactions[1];
        assert!(
            tx.effects
                .mutated()
                .iter()
                .any(|((id, ..), owner)| *id == obj_id
                    && owner.get_owner_address().unwrap()
                        == TestCheckpointBuilder::derive_address(1))
        );
    }

    #[test]
    fn test_shared_object() {
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .create_shared_object(0)
            .finish_transaction()
            .build_checkpoint();

        let obj_id = TestCheckpointBuilder::derive_object_id(0);

        // Verify object is in object set and is shared
        assert!(
            checkpoint
                .object_set
                .iter()
                .any(|obj| obj.id() == obj_id && obj.owner().is_shared())
        );
    }

    #[test]
    fn test_freeze_object() {
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction()
            .start_transaction(0)
            .change_object_owner(0, Owner::Immutable)
            .finish_transaction()
            .build_checkpoint();

        let obj_id = TestCheckpointBuilder::derive_object_id(0);

        // Verify object is in object set and is immutable
        assert!(
            checkpoint
                .object_set
                .iter()
                .any(|obj| obj.id() == obj_id && obj.owner().is_immutable())
        );
    }

    #[test]
    fn test_sui_balance_transfer() {
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .create_sui_object(0, 100)
            .finish_transaction()
            .start_transaction(1)
            .transfer_coin_balance(0, 1, 1, 10)
            .finish_transaction()
            .build_checkpoint();

        let obj_id0 = TestCheckpointBuilder::derive_object_id(0);
        let obj_id1 = TestCheckpointBuilder::derive_object_id(1);

        // Verify both coins are in the final object set with correct balances
        assert!(checkpoint.object_set.iter().any(|obj| obj.id() == obj_id0
            && obj.is_gas_coin()
            && obj.data.try_as_move().unwrap().get_coin_value_unsafe() == 90));

        assert!(checkpoint.object_set.iter().any(|obj| obj.id() == obj_id1
            && obj.is_gas_coin()
            && obj.data.try_as_move().unwrap().get_coin_value_unsafe() == 10));
    }

    #[test]
    fn test_coin_balance_transfer() {
        let type_tag = TypeTag::from_str("0x100::a::b").unwrap();
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .create_coin_object(0, 0, 100, type_tag.clone())
            .finish_transaction()
            .start_transaction(1)
            .transfer_coin_balance(0, 1, 1, 10)
            .finish_transaction()
            .build_checkpoint();

        let obj_id0 = TestCheckpointBuilder::derive_object_id(0);
        let obj_id1 = TestCheckpointBuilder::derive_object_id(1);

        // Verify both coins are in the final object set with correct balances
        assert!(checkpoint.object_set.iter().any(|obj| obj.id() == obj_id0
            && obj.coin_type_maybe().unwrap() == type_tag
            && obj.data.try_as_move().unwrap().get_coin_value_unsafe() == 90));

        assert!(checkpoint.object_set.iter().any(|obj| obj.id() == obj_id1
            && obj.coin_type_maybe().unwrap() == type_tag
            && obj.data.try_as_move().unwrap().get_coin_value_unsafe() == 10));
    }

    #[test]
    fn test_events() {
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .with_events(vec![Event::new(
                &ObjectID::ZERO,
                ident_str!("test"),
                TestCheckpointBuilder::derive_address(0),
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
        let checkpoint = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .add_move_call(ObjectID::ZERO, "test", "test")
            .finish_transaction()
            .build_checkpoint();
        let tx = &checkpoint.transactions[0];

        // Verify the transaction has a move call matching the arguments provided.
        assert!(tx.transaction.kind().iter_commands().any(|cmd| {
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
        let mut builder = TestCheckpointBuilder::new(1)
            .start_transaction(0)
            .create_owned_object(0)
            .finish_transaction();
        let checkpoint1 = builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .mutate_owned_object(0)
            .finish_transaction();
        let checkpoint2 = builder.build_checkpoint();
        builder = builder
            .start_transaction(0)
            .delete_object(0)
            .finish_transaction();
        let checkpoint3 = builder.build_checkpoint();

        // Verify the sequence numbers are consecutive.
        assert_eq!(checkpoint1.summary.sequence_number, 1);
        assert_eq!(checkpoint2.summary.sequence_number, 2);
        assert_eq!(checkpoint3.summary.sequence_number, 3);
    }
}
