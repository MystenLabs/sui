// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    schema::{
        event_emit_module, event_emit_package, event_senders, event_struct_instantiation,
        event_struct_module, event_struct_name, event_struct_package,
    },
    types::EventIndex,
};
use diesel::prelude::*;

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = event_emit_package)]
pub struct StoredEventEmitPackage {
    pub tx_sequence_number: i64,
    pub event_sequence_number: i64,
    pub package: Vec<u8>,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = event_emit_module)]
pub struct StoredEventEmitModule {
    pub tx_sequence_number: i64,
    pub event_sequence_number: i64,
    pub package: Vec<u8>,
    pub module: String,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = event_senders)]
pub struct StoredEventSenders {
    pub tx_sequence_number: i64,
    pub event_sequence_number: i64,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = event_struct_package)]
pub struct StoredEventStructPackage {
    pub tx_sequence_number: i64,
    pub event_sequence_number: i64,
    pub package: Vec<u8>,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = event_struct_module)]
pub struct StoredEventStructModule {
    pub tx_sequence_number: i64,
    pub event_sequence_number: i64,
    pub package: Vec<u8>,
    pub module: String,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = event_struct_name)]
pub struct StoredEventStructName {
    pub tx_sequence_number: i64,
    pub event_sequence_number: i64,
    pub package: Vec<u8>,
    pub module: String,
    pub type_name: String,
    pub sender: Vec<u8>,
}

#[derive(Queryable, Insertable, Selectable, Debug, Clone, Default)]
#[diesel(table_name = event_struct_instantiation)]
pub struct StoredEventStructInstantiation {
    pub tx_sequence_number: i64,
    pub event_sequence_number: i64,
    pub package: Vec<u8>,
    pub module: String,
    pub type_instantiation: String,
    pub sender: Vec<u8>,
}

impl EventIndex {
    pub fn split(
        self: EventIndex,
    ) -> (
        StoredEventEmitPackage,
        StoredEventEmitModule,
        StoredEventSenders,
        StoredEventStructPackage,
        StoredEventStructModule,
        StoredEventStructName,
        StoredEventStructInstantiation,
    ) {
        let tx_sequence_number = self.tx_sequence_number as i64;
        let event_sequence_number = self.event_sequence_number as i64;
        (
            StoredEventEmitPackage {
                tx_sequence_number,
                event_sequence_number,
                package: self.emit_package.to_vec(),
                sender: self.sender.to_vec(),
            },
            StoredEventEmitModule {
                tx_sequence_number,
                event_sequence_number,
                package: self.emit_package.to_vec(),
                module: self.emit_module.clone(),
                sender: self.sender.to_vec(),
            },
            StoredEventSenders {
                tx_sequence_number,
                event_sequence_number,
                sender: self.sender.to_vec(),
            },
            StoredEventStructPackage {
                tx_sequence_number,
                event_sequence_number,
                package: self.type_package.to_vec(),
                sender: self.sender.to_vec(),
            },
            StoredEventStructModule {
                tx_sequence_number,
                event_sequence_number,
                package: self.type_package.to_vec(),
                module: self.type_module.clone(),
                sender: self.sender.to_vec(),
            },
            StoredEventStructName {
                tx_sequence_number,
                event_sequence_number,
                package: self.type_package.to_vec(),
                module: self.type_module.clone(),
                type_name: self.type_name.clone(),
                sender: self.sender.to_vec(),
            },
            StoredEventStructInstantiation {
                tx_sequence_number,
                event_sequence_number,
                package: self.type_package.to_vec(),
                module: self.type_module.clone(),
                type_instantiation: self.type_instantiation.clone(),
                sender: self.sender.to_vec(),
            },
        )
    }
}
