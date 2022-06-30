// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::benchmark::validator_preparer::{get_multithread_runtime, ValidatorPreparer};
use move_core_types::{account_address::AccountAddress, ident_str};
use rayon::prelude::*;
use sui_config::NetworkConfig;
use sui_types::{
    base_types::*,
    crypto::{AuthoritySignature, KeyPair, Signature},
    messages::*,
    object::Object,
    SUI_FRAMEWORK_ADDRESS,
};

const OBJECT_ID_OFFSET: &str = "0x10000";
const GAS_PER_TX: u64 = u64::MAX;

/// Create a transaction for object transfer
/// This can either use the Move path or the native path
fn make_transfer_transaction(
    object_ref: ObjectRef,
    recipient: SuiAddress,
    use_move: bool,
) -> SingleTransactionKind {
    if use_move {
        let framework_obj_ref = (
            ObjectID::from(SUI_FRAMEWORK_ADDRESS),
            SequenceNumber::new(),
            ObjectDigest::new([0; 32]),
        );

        SingleTransactionKind::Call(MoveCall {
            package: framework_obj_ref,
            module: ident_str!("sui").to_owned(),
            function: ident_str!("transfer").to_owned(),
            type_arguments: Vec::new(),
            arguments: vec![
                CallArg::Object(ObjectArg::ImmOrOwnedObject(object_ref)),
                CallArg::Pure(bcs::to_bytes(&AccountAddress::from(recipient)).unwrap()),
            ],
        })
    } else {
        SingleTransactionKind::TransferObject(TransferObject {
            recipient,
            object_ref,
        })
    }
}

/// Creates an object for use in the microbench
fn create_gas_object(object_id: ObjectID, owner: SuiAddress) -> Object {
    Object::with_id_owner_gas_for_testing(object_id, owner, GAS_PER_TX)
}

/// This builds, signs a cert
fn make_cert(network_config: &NetworkConfig, tx: &Transaction) -> CertifiedTransaction {
    // Make certificate
    let committee = network_config.committee();
    // TODO: Why iterating from 0 to quorum_threshold??
    let mut signatures: Vec<(AuthorityName, AuthoritySignature)> = Vec::new();
    for i in 0..committee.quorum_threshold() {
        let secx = network_config
            .validator_configs()
            .get(i as usize)
            .unwrap()
            .key_pair();
        let pubx = secx.public_key_bytes();
        let sig = AuthoritySignature::new(&tx.data, secx);
        signatures.push((*pubx, sig));
    }
    CertifiedTransaction::new_with_signatures(committee.epoch(), tx.clone(), signatures).unwrap()
}

fn make_transactions(
    address: SuiAddress,
    keypair: KeyPair,
    network_config: &NetworkConfig,
    account_gas_objects: &[(Vec<Object>, Object)],
    batch_size: usize,
    use_move: bool,
) -> Vec<(Transaction, CertifiedTransaction)> {
    // Make one transaction per account
    // Depending on benchmark_type, this could be the Order and/or Confirmation.
    account_gas_objects
        .par_iter()
        .map(|(objects, gas_obj)| {
            let next_recipient: SuiAddress = KeyPair::get_key_pair().0;
            let mut single_kinds = vec![];
            for object in objects {
                single_kinds.push(make_transfer_transaction(
                    object.compute_object_reference(),
                    next_recipient,
                    use_move,
                ));
            }
            let gas_object_ref = gas_obj.compute_object_reference();
            let data = if batch_size == 1 {
                TransactionData::new(
                    TransactionKind::Single(single_kinds.into_iter().next().unwrap()),
                    address,
                    gas_object_ref,
                    10000,
                )
            } else {
                assert!(single_kinds.len() == batch_size, "Inconsistent batch size");
                TransactionData::new(
                    TransactionKind::Batch(single_kinds),
                    address,
                    gas_object_ref,
                    2000000,
                )
            };

            let signature = Signature::new(&data, &keypair);
            let transaction = Transaction::new(data, signature);
            let cert = make_cert(network_config, &transaction);

            (transaction, cert)
        })
        .collect()
}

pub struct TransactionCreator {
    pub object_id_offset: ObjectID,
}

impl Default for TransactionCreator {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionCreator {
    pub fn new() -> Self {
        Self {
            object_id_offset: ObjectID::from_hex_literal(OBJECT_ID_OFFSET).unwrap(),
        }
    }
    pub fn new_with_offset(object_id_offset: ObjectID) -> Self {
        Self { object_id_offset }
    }

    pub fn generate_transactions(
        &mut self,
        tcp_conns: usize,
        use_move: bool,
        chunk_size: usize,
        num_chunks: usize,
        sender: Option<&KeyPair>,
        validator_preparer: &mut ValidatorPreparer,
    ) -> Vec<(Transaction, CertifiedTransaction)> {
        let (address, keypair) = if let Some(a) = sender {
            (SuiAddress::from(a.public_key_bytes()), a.copy())
        } else {
            KeyPair::get_key_pair()
        };
        let (transactions, txn_objects) = self.make_transactions(
            address,
            keypair,
            chunk_size,
            num_chunks,
            tcp_conns,
            use_move,
            self.object_id_offset,
            &validator_preparer.network_config,
        );

        get_multithread_runtime().block_on(async move {
            validator_preparer
                .update_objects_for_validator(txn_objects, address)
                .await;
        });

        transactions
    }

    fn make_gas_objects(
        &mut self,
        address: SuiAddress,
        tx_count: usize,
        batch_size: usize,
        obj_id_offset: ObjectID,
    ) -> Vec<(Vec<Object>, Object)> {
        let total_count = tx_count * batch_size;
        let mut objects = vec![];
        let mut gas_objects = vec![];
        // Objects to be transferred
        ObjectID::in_range(obj_id_offset, total_count as u64)
            .unwrap()
            .iter()
            .for_each(|q| objects.push(create_gas_object(*q, address)));

        // Objects for payment
        let next_offset = objects[objects.len() - 1].id();

        ObjectID::in_range(next_offset.next_increment().unwrap(), tx_count as u64)
            .unwrap()
            .iter()
            .for_each(|q| gas_objects.push(create_gas_object(*q, address)));

        self.object_id_offset = gas_objects[gas_objects.len() - 1]
            .id()
            .next_increment()
            .unwrap();

        objects[..]
            .chunks(batch_size)
            .into_iter()
            .map(|q| q.to_vec())
            .zip(gas_objects.into_iter())
            .collect::<Vec<_>>()
    }

    fn make_transactions(
        &mut self,
        address: SuiAddress,
        key_pair: KeyPair,
        chunk_size: usize,
        num_chunks: usize,
        conn: usize,
        use_move: bool,
        object_id_offset: ObjectID,
        network_config: &NetworkConfig,
    ) -> (Vec<(Transaction, CertifiedTransaction)>, Vec<Object>) {
        assert_eq!(chunk_size % conn, 0);
        let batch_size_per_conn = chunk_size / conn;

        // The batch-adjusted number of transactions
        let batch_tx_count = num_chunks * conn;
        // Only need one gas object per batch
        let account_gas_objects: Vec<_> = self.make_gas_objects(
            address,
            batch_tx_count,
            batch_size_per_conn,
            object_id_offset,
        );

        // Bulk load objects
        let all_objects: Vec<_> = account_gas_objects
            .clone()
            .into_iter()
            .flat_map(|(objects, gas)| objects.into_iter().chain(std::iter::once(gas)))
            .collect();

        let transactions = make_transactions(
            address,
            key_pair,
            network_config,
            &account_gas_objects,
            batch_size_per_conn,
            use_move,
        );
        (transactions, all_objects)
    }
}
