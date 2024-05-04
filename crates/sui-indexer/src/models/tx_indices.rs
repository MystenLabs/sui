// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use crate::{
    schema::{
        tx_calls, tx_changed_objects, tx_digests, tx_input_objects, tx_recipients, tx_senders,
    },
    types::TxIndex,
};
use diesel::prelude::*;
use sui_types::base_types::SuiAddress;

#[derive(QueryableByName)]
pub struct TxSequenceNumber {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub tx_sequence_number: i64,
}

#[derive(QueryableByName)]
pub struct TxDigest {
    #[diesel(sql_type = diesel::sql_types::Binary)]
    pub transaction_digest: Vec<u8>,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_senders)]
pub struct StoredTxSenders {
    pub cp_sequence_number: i64,
    pub tx_sequence_number: i64,
    pub sender: Vec<u8>,
    pub transaction_kind: i16,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_recipients)]
pub struct StoredTxRecipients {
    pub cp_sequence_number: i64,
    pub tx_sequence_number: i64,
    pub recipient: Vec<u8>,
    pub transaction_kind: i16,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_input_objects)]
pub struct StoredTxInputObject {
    pub cp_sequence_number: i64,
    pub tx_sequence_number: i64,
    pub object_id: Vec<u8>,
    pub address: Vec<u8>,
    pub rel: i16,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_changed_objects)]
pub struct StoredTxChangedObject {
    pub cp_sequence_number: i64,
    pub tx_sequence_number: i64,
    pub object_id: Vec<u8>,
    pub address: Vec<u8>,
    pub rel: i16,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_calls)]
pub struct StoredTxCalls {
    pub cp_sequence_number: i64,
    pub tx_sequence_number: i64,
    pub package: Vec<u8>,
    pub module: String,
    pub func: String,
    pub address: Vec<u8>,
    pub rel: i16,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_digests)]
pub struct StoredTxDigest {
    pub tx_digest: Vec<u8>,
    pub tx_sequence_number: i64,
    pub cp_sequence_number: i64,
}

#[allow(clippy::type_complexity)]
impl TxIndex {
    pub fn split(
        self: TxIndex,
    ) -> (
        Vec<StoredTxSenders>,
        Vec<StoredTxRecipients>,
        Vec<StoredTxInputObject>,
        Vec<StoredTxChangedObject>,
        Vec<StoredTxCalls>,
        Vec<StoredTxDigest>,
    ) {
        let tx_sequence_number = self.tx_sequence_number as i64;
        let cp_sequence_number = self.checkpoint_sequence_number as i64;
        let transaction_kind = self.transaction_kind as i16;
        let tx_senders = self
            .senders
            .iter()
            .map(|s| StoredTxSenders {
                cp_sequence_number,
                tx_sequence_number,
                sender: s.to_vec(),
                transaction_kind,
            })
            .collect();
        let tx_recipients = self
            .recipients
            .iter()
            .map(|s| StoredTxRecipients {
                cp_sequence_number,
                tx_sequence_number,
                recipient: s.to_vec(),
                transaction_kind,
            })
            .collect();

        let mut address_rel_map: HashMap<&SuiAddress, i16> = HashMap::new();
        for sender in &self.senders {
            address_rel_map.entry(sender).or_insert(0);
        }

        for recipient in &self.recipients {
            address_rel_map
                .entry(recipient)
                .and_modify(|e| {
                    if *e == 0 {
                        *e = 1
                    }
                })
                .or_insert(2);
        }

        let mut tx_input_objects = Vec::new();

        for tx_input_object in self.input_objects {
            for (address, rel) in &address_rel_map {
                tx_input_objects.push(StoredTxInputObject {
                    cp_sequence_number,
                    tx_sequence_number,
                    object_id: bcs::to_bytes(&tx_input_object).unwrap(),
                    address: address.to_vec(),
                    rel: *rel,
                });
            }
        }

        let mut tx_changed_objects = Vec::new();
        for tx_changed_object in self.changed_objects {
            for (address, rel) in &address_rel_map {
                tx_changed_objects.push(StoredTxChangedObject {
                    cp_sequence_number,
                    tx_sequence_number,
                    object_id: bcs::to_bytes(&tx_changed_object).unwrap(),
                    address: address.to_vec(),
                    rel: *rel,
                });
            }
        }

        let mut tx_calls = Vec::new();
        for (p, m, f) in self.move_calls {
            for (address, rel) in &address_rel_map {
                tx_calls.push(StoredTxCalls {
                    cp_sequence_number,
                    tx_sequence_number,
                    package: p.to_vec(),
                    module: m.to_string(),
                    func: f.to_string(),
                    address: address.to_vec(),
                    rel: *rel,
                });
            }
        }

        let stored_tx_digest = StoredTxDigest {
            tx_digest: self.transaction_digest.into_inner().to_vec(),
            tx_sequence_number,
            cp_sequence_number,
        };

        (
            tx_senders,
            tx_recipients,
            tx_input_objects,
            tx_changed_objects,
            tx_calls,
            vec![stored_tx_digest],
        )
    }
}
