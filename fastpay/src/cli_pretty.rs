// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use std::cmp;

use fastpay::config::InitialStateConfig;
use fastx_types::{
    base_types::{encode_address_hex, SequenceNumber},
    messages::{ExecutionStatus, ObjectInfoResponse, OrderEffects},
};
use move_core_types::account_address::AccountAddress;
use prettytable::{cell, format, row, Table};

pub fn format_obj_info_response(obj_info: &ObjectInfoResponse) -> Table {
    let mut tbl = Table::new();
    tbl.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);

    let type_str = match obj_info.object.data.type_() {
        Some(v) => format!("{}", v),
        None => "N/A".to_owned(),
    };

    tbl.set_titles(row!["ID", "Owner", "Version", "Type", "Readonly"]);
    tbl.add_row(row![
        obj_info.object.id(),
        encode_address_hex(&obj_info.object.owner),
        u64::from(obj_info.object.version()),
        type_str,
        obj_info.object.is_read_only()
    ]);

    tbl
}

pub fn format_order_effects(order_effetcs: &OrderEffects) -> Table {
    let mut tbl = Table::new();
    tbl.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);
    tbl.set_titles(row![
        "Execution Success",
        "Mutated Objects",
        "Deleted Objects"
    ]);

    let mut mut_table = Table::new();
    mut_table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

    let mut del_table = Table::new();
    del_table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

    mut_table.set_titles(row!["ObjectID", "Version", "Owner"]);
    del_table.set_titles(row!["ObjectID", "Version"]);

    for idx in 0..cmp::max(order_effetcs.deleted.len(), order_effetcs.mutated.len()) {
        let del_str = order_effetcs
            .deleted
            .get(idx)
            .map(|w| (format!("{:?}", w.0), format!("{:?}", u64::from(w.1))))
            .unwrap_or_else(|| ("".to_string(), "".to_string()));
        let mut_str = order_effetcs
            .mutated
            .get(idx)
            .map(|w| {
                (
                    format!("{:?}", w.0 .0),
                    format!("{:?}", u64::from(w.0 .1)),
                    format!("{:?}", encode_address_hex(&w.1)),
                )
            })
            .unwrap_or_else(|| ("".to_string(), "".to_string(), "".to_string()));
        if !mut_str.0.is_empty() {
            mut_table.add_row(row![mut_str.0, mut_str.1, mut_str.2]);
        }
        if !del_str.0.is_empty() {
            del_table.add_row(row![del_str.0, del_str.1]);
        }
    }
    tbl.add_row(row![
        order_effetcs.status == ExecutionStatus::Success,
        mut_table,
        del_table
    ]);
    tbl
}

pub fn format_objects(obj_map: &[(AccountAddress, SequenceNumber)]) -> Table {
    let mut tbl = Table::new();
    tbl.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);

    tbl.set_titles(row!["ObjectID", "Version"]);
    for (obj_id, seq_no) in obj_map {
        tbl.add_row(row![obj_id.to_hex(), u64::from(*seq_no)]);
    }

    tbl
}

pub fn format_account_configs_create(acc_cfgs: InitialStateConfig) -> Table {
    let mut tbl = Table::new();
    tbl.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

    tbl.set_titles(row!["Address", "Object Info"]);

    for a in acc_cfgs.config {
        let mut obj_table = Table::new();
        obj_table.set_format(*format::consts::FORMAT_NO_LINESEP_WITH_TITLE);

        obj_table.set_titles(row!["ObjectID", "Gas Value"]);
        for (obj_id, gas_val) in a.object_ids_and_gas_vals {
            obj_table.add_row(row![obj_id, gas_val]);
        }
        tbl.add_row(row![encode_address_hex(&a.address), obj_table]);
    }
    tbl
}
