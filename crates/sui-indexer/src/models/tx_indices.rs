// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    schema::{tx_calls, tx_changed_objects, tx_input_objects, tx_recipients, tx_senders},
    types::TxIndex,
};
use diesel::prelude::*;

#[derive(QueryableByName)]
pub struct TxSequenceNumber {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    pub tx_sequence_number: i64,
}

#[derive(QueryableByName)]
pub struct TxDigest {
    #[diesel(sql_type = diesel::sql_types::Bytea)]
    pub transaction_digest: Vec<u8>,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_input_objects)]
pub struct StoredTxInputObject {
    pub tx_sequence_number: i64,
    pub object_id: Vec<u8>,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_changed_objects)]
pub struct StoredTxChangedObject {
    pub tx_sequence_number: i64,
    pub object_id: Vec<u8>,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_senders)]
pub struct StoredTxSenders {
    pub tx_sequence_number: i64,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_recipients)]
pub struct StoredTxRecipients {
    pub tx_sequence_number: i64,
    pub recipient: Vec<u8>,
}

#[derive(Queryable, Insertable, Debug, Clone, Default)]
#[diesel(table_name = tx_calls)]
pub struct StoredTxCalls {
    pub tx_sequence_number: i64,
    pub package: Vec<u8>,
    pub module: String,
    pub func: String,
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
    ) {
        let tx_sequence_number = self.tx_sequence_number as i64;
        let tx_senders = self
            .senders
            .iter()
            .map(|s| StoredTxSenders {
                tx_sequence_number,
                sender: s.to_vec(),
            })
            .collect();
        let tx_recipients = self
            .recipients
            .iter()
            .map(|s| StoredTxRecipients {
                tx_sequence_number,
                recipient: s.to_vec(),
            })
            .collect();
        let tx_input_objects = self
            .input_objects
            .iter()
            .map(|o| StoredTxInputObject {
                tx_sequence_number,
                object_id: bcs::to_bytes(&o).unwrap(),
            })
            .collect();
        let tx_changed_objects = self
            .changed_objects
            .iter()
            .map(|o| StoredTxChangedObject {
                tx_sequence_number,
                object_id: bcs::to_bytes(&o).unwrap(),
            })
            .collect();
        let tx_calls = self
            .move_calls
            .iter()
            .map(|(p, m, f)| StoredTxCalls {
                tx_sequence_number,
                package: p.to_vec(),
                module: m.to_string(),
                func: f.to_string(),
            })
            .collect();
        (
            tx_senders,
            tx_recipients,
            tx_input_objects,
            tx_changed_objects,
            tx_calls,
        )
    }
}
