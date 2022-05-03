// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::benchmark::validator_preparer::ValidatorPreparer;
use bytes::Bytes;
use move_core_types::account_address::AccountAddress;
use move_core_types::ident_str;
use rayon::prelude::*;
use sui_types::crypto::{get_key_pair, AuthoritySignature, KeyPair, Signature};
use sui_types::SUI_FRAMEWORK_ADDRESS;
use sui_types::{base_types::*, committee::*, messages::*, object::Object, serialize::*};

const OBJECT_ID_OFFSET: usize = 10000;

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
            module: ident_str!("SUI").to_owned(),
            function: ident_str!("transfer").to_owned(),
            type_arguments: Vec::new(),
            arguments: vec![
                CallArg::ImmOrOwnedObject(object_ref),
                CallArg::Pure(bcs::to_bytes(&AccountAddress::from(recipient)).unwrap()),
            ],
        })
    } else {
        SingleTransactionKind::Transfer(Transfer {
            recipient,
            object_ref,
        })
    }
}

/// Creates an object for use in the microbench
fn create_object(object_id: ObjectID, owner: SuiAddress, use_move: bool) -> Object {
    if use_move {
        Object::with_id_owner_gas_coin_object_for_testing(
            object_id,
            SequenceNumber::new(),
            owner,
            1,
        )
    } else {
        Object::with_id_owner_for_testing(object_id, owner)
    }
}

/// This builds, signs a cert and serializes it
fn make_serialized_cert(keys: &[KeyPair], committee: &Committee, tx: Transaction) -> Vec<u8> {
    // Make certificate
    let mut certificate = CertifiedTransaction::new(tx);
    certificate.auth_sign_info.epoch = committee.epoch();
    for i in 0..committee.quorum_threshold() {
        let secx = keys.get(i).unwrap();
        let pubx = secx.public_key_bytes();
        let sig = AuthoritySignature::new(&certificate.data, secx);
        certificate.auth_sign_info.signatures.push((*pubx, sig));
    }

    let serialized_certificate = serialize_cert(&certificate);
    assert!(!serialized_certificate.is_empty());
    serialized_certificate
}

fn make_gas_objects(
    address: SuiAddress,
    tx_count: usize,
    batch_size: usize,
    obj_id_offset: usize,
    use_move: bool,
) -> Vec<(Vec<Object>, Object)> {
    (0..tx_count)
        .into_par_iter()
        .map(|x| {
            let mut objects = vec![];
            for i in 0..batch_size {
                let mut obj_id = [0; 20];
                obj_id[..8]
                    .clone_from_slice(&(obj_id_offset + x * batch_size + i).to_be_bytes()[..8]);
                objects.push(create_object(ObjectID::from(obj_id), address, use_move));
            }

            let mut gas_object_id = [0; 20];
            gas_object_id[8..16].clone_from_slice(&(obj_id_offset + x).to_be_bytes()[..8]);
            let gas_object = Object::with_id_owner_gas_coin_object_for_testing(
                ObjectID::from(gas_object_id),
                SequenceNumber::new(),
                address,
                2000000,
            );
            assert!(gas_object.version() == SequenceNumber::from(0));

            (objects, gas_object)
        })
        .collect()
}

fn make_serialized_transactions(
    address: SuiAddress,
    keypair: KeyPair,
    committee: &Committee,
    account_gas_objects: &[(Vec<Object>, Object)],
    authority_keys: &[KeyPair],
    batch_size: usize,
    use_move: bool,
) -> Vec<Bytes> {
    // Make one transaction per account
    // Depending on benchmark_type, this could be the Order and/or Confirmation.
    account_gas_objects
        .par_iter()
        .map(|(objects, gas_obj)| {
            let next_recipient: SuiAddress = get_key_pair().0;
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

            // Serialize transaction
            let serialized_transaction = serialize_transaction(&transaction);

            assert!(!serialized_transaction.is_empty());

            vec![
                serialized_transaction.into(),
                make_serialized_cert(authority_keys, committee, transaction).into(),
            ]
        })
        .flatten()
        .collect()
}

fn make_transactions(
    address: SuiAddress,
    key_pair: KeyPair,
    chunk_size: usize,
    num_chunks: usize,
    conn: usize,
    use_move: bool,
    object_id_offset: usize,
    auth_keys: &[KeyPair],
    committee: &Committee,
) -> (Vec<Bytes>, Vec<Object>) {
    assert_eq!(chunk_size % conn, 0);
    let batch_size_per_conn = chunk_size / conn;

    // The batch-adjusted number of transactions
    let batch_tx_count = num_chunks * conn;
    // Only need one gas object per batch
    let account_gas_objects: Vec<_> = make_gas_objects(
        address,
        batch_tx_count,
        batch_size_per_conn,
        object_id_offset,
        use_move,
    );

    // Bulk load objects
    let all_objects: Vec<_> = account_gas_objects
        .clone()
        .into_iter()
        .flat_map(|(objects, gas)| objects.into_iter().chain(std::iter::once(gas)))
        .collect();

    let serialized_txes = make_serialized_transactions(
        address,
        key_pair,
        committee,
        &account_gas_objects,
        auth_keys,
        batch_size_per_conn,
        use_move,
    );
    (serialized_txes, all_objects)
}

pub struct TransactionCreator {
    pub object_id_offset: usize,
}

impl Default for TransactionCreator {
    fn default() -> Self {
        Self::new()
    }
}

impl TransactionCreator {
    pub fn new() -> Self {
        Self {
            object_id_offset: OBJECT_ID_OFFSET,
        }
    }

    pub fn generate_transactions(
        &mut self,
        tcp_conns: usize,
        use_move: bool,
        chunk_size: usize,
        num_chunks: usize,
        validator_preparer: &mut ValidatorPreparer,
    ) -> Vec<Bytes> {
        let (address, keypair) = get_key_pair();
        let (signed_txns, txn_objects) = make_transactions(
            address,
            keypair,
            chunk_size,
            num_chunks,
            tcp_conns,
            use_move,
            self.object_id_offset,
            &validator_preparer.keys,
            &validator_preparer.committee,
        );

        self.object_id_offset += chunk_size * num_chunks;

        validator_preparer.update_objects_for_validator(txn_objects, address);

        signed_txns
    }
}
