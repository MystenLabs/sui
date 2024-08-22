// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    schema::{
        tx_calls_fun, tx_calls_mod, tx_calls_pkg, tx_changed_objects, tx_digests, tx_input_objects,
        tx_kinds, tx_recipients, tx_senders,
    },
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

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = tx_senders)]
pub struct StoredTxSenders {
    pub tx_sequence_number: i64,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = tx_recipients)]
pub struct StoredTxRecipients {
    pub tx_sequence_number: i64,
    pub recipient: Vec<u8>,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = tx_input_objects)]
pub struct StoredTxInputObject {
    pub tx_sequence_number: i64,
    pub object_id: Vec<u8>,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = tx_changed_objects)]
pub struct StoredTxChangedObject {
    pub tx_sequence_number: i64,
    pub object_id: Vec<u8>,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = tx_calls_pkg)]
pub struct StoredTxPkg {
    pub tx_sequence_number: i64,
    pub package: Vec<u8>,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = tx_calls_mod)]
pub struct StoredTxMod {
    pub tx_sequence_number: i64,
    pub package: Vec<u8>,
    pub module: String,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = tx_calls_fun)]
pub struct StoredTxFun {
    pub tx_sequence_number: i64,
    pub package: Vec<u8>,
    pub module: String,
    pub func: String,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = tx_digests)]
pub struct StoredTxDigest {
    pub tx_digest: Vec<u8>,
    pub tx_sequence_number: i64,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = tx_kinds)]
pub struct StoredTxKind {
    pub tx_kind: i16,
    pub tx_sequence_number: i64,
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
        Vec<StoredTxPkg>,
        Vec<StoredTxMod>,
        Vec<StoredTxFun>,
        Vec<StoredTxDigest>,
        Vec<StoredTxKind>,
    ) {
        let tx_sequence_number = self.tx_sequence_number as i64;
        let tx_sender = StoredTxSenders {
            tx_sequence_number,
            sender: self.sender.to_vec(),
        };
        let tx_recipients = self
            .recipients
            .iter()
            .map(|s| StoredTxRecipients {
                tx_sequence_number,
                recipient: s.to_vec(),
                sender: self.sender.to_vec(),
            })
            .collect();
        let tx_input_objects = self
            .input_objects
            .iter()
            .map(|o| StoredTxInputObject {
                tx_sequence_number,
                object_id: bcs::to_bytes(&o).unwrap(),
                sender: self.sender.to_vec(),
            })
            .collect();
        let tx_changed_objects = self
            .changed_objects
            .iter()
            .map(|o| StoredTxChangedObject {
                tx_sequence_number,
                object_id: bcs::to_bytes(&o).unwrap(),
                sender: self.sender.to_vec(),
            })
            .collect();

        let mut packages = Vec::new();
        let mut packages_modules = Vec::new();
        let mut packages_modules_funcs = Vec::new();

        for (pkg, pkg_mod, pkg_mod_func) in self
            .move_calls
            .iter()
            .map(|(p, m, f)| (*p, (*p, m.clone()), (*p, m.clone(), f.clone())))
        {
            packages.push(pkg);
            packages_modules.push(pkg_mod);
            packages_modules_funcs.push(pkg_mod_func);
        }

        let tx_pkgs = packages
            .iter()
            .map(|p| StoredTxPkg {
                tx_sequence_number,
                package: p.to_vec(),
                sender: self.sender.to_vec(),
            })
            .collect();

        let tx_mods = packages_modules
            .iter()
            .map(|(p, m)| StoredTxMod {
                tx_sequence_number,
                package: p.to_vec(),
                module: m.to_string(),
                sender: self.sender.to_vec(),
            })
            .collect();

        let tx_calls = packages_modules_funcs
            .iter()
            .map(|(p, m, f)| StoredTxFun {
                tx_sequence_number,
                package: p.to_vec(),
                module: m.to_string(),
                func: f.to_string(),
                sender: self.sender.to_vec(),
            })
            .collect();

        let stored_tx_digest = StoredTxDigest {
            tx_digest: self.transaction_digest.into_inner().to_vec(),
            tx_sequence_number,
        };

        let tx_kind = StoredTxKind {
            tx_kind: self.tx_kind as i16,
            tx_sequence_number,
        };

        (
            vec![tx_sender],
            tx_recipients,
            tx_input_objects,
            tx_changed_objects,
            tx_pkgs,
            tx_mods,
            tx_calls,
            vec![stored_tx_digest],
            vec![tx_kind],
        )
    }
}
