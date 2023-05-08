// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_bytecode_verifier::meter::BoundMeter;
use prometheus::Registry;
use std::path::PathBuf;
use std::sync::Arc;
use sui_adapter::adapter::default_verifier_config;
use sui_adapter::adapter::run_metered_move_bytecode_verifier_impl;
use sui_core::authority::authority_store_tables::{
    AuthorityPerpetualTables, AuthorityPerpetualTablesReadOnly,
};
use sui_framework::BuiltInFramework;
use sui_protocol_config::ProtocolConfig;
use sui_storage::indexes::IndexStoreTablesReadOnly;
use sui_storage::IndexStoreTables;
use sui_types::metrics::BytecodeVerifierMetrics;
use typed_store::rocks::MetricConf;
pub struct _DBRider {
    pub index_store: IndexStoreTablesReadOnly,
    pub perpatual_store: AuthorityPerpetualTablesReadOnly,
}

impl _DBRider {
    pub fn _open(path: PathBuf) -> Self {
        let mut index_path = path.clone();
        index_path.push("indexes");

        let index_store =
            IndexStoreTables::get_read_only_handle(index_path, None, None, MetricConf::default());
        let mut perpetual_path = path;
        perpetual_path.push("store");
        perpetual_path.push("perpetual");

        let perpatual_store = AuthorityPerpetualTables::get_read_only_handle(
            perpetual_path,
            None,
            None,
            MetricConf::default(),
        );
        Self {
            index_store,
            perpatual_store,
        }
    }
}

use move_bytecode_verifier::meter::{BoundMeter, Scope};
use std::fs::File;
use std::io::{BufWriter, Write};
use std::time::Instant;
use sui_core::authority::authority_store_types::StoreData;
use sui_core::authority::authority_store_types::StoreObjectWrapper;
use sui_core::authority::authority_store_types::{StoreObjectV1, StoreObjectValue};
use sui_types::object::Data;
use sui_types::object::Object;
use sui_types::storage::ObjectKey;
use typed_store::Map;

pub fn check_packages(path: PathBuf, out_file: String) {
    println!(
        "Will try to open db at {:?}, and write to {}",
        path, out_file
    );

    let mut perpetual_path = path;

    perpetual_path.push("perpetual");

    let perpatual_store = AuthorityPerpetualTables::get_read_only_handle(
        perpetual_path,
        None,
        None,
        MetricConf::default(),
    );
    println!("Will try to catch up with primary");
    perpatual_store.objects.try_catch_up_with_primary().unwrap();

    let packages = perpatual_store
        .objects
        .safe_iter()
        .filter_map(|en| match en {
            Ok((key, value)) => try_construct_package_object(&key, &value),
            Err(_) => None,
        });

    let protocol_config = ProtocolConfig::get_for_max_version();
    let metered_verifier_config =
        default_verifier_config(&protocol_config, true /* enable metering */);
    let registry = &Registry::new();
    let bytecode_verifier_metrics = Arc::new(BytecodeVerifierMetrics::new(registry));
    let mut num_packages = 0u64;

    let f = File::create(out_file).expect("Unable to create file");
    let mut f = BufWriter::new(f);
    println!("Will process {} packages", num_packages);
    for package in packages {
        match package.data {
            Data::Package(package) => {
                num_packages += 1;
                let modules = package.deserialize_modules(6, false).unwrap();

                // Use the same meter for all packages
                let mut meter = BoundMeter::new(&metered_verifier_config);
                let start = Instant::now();
                let status = run_metered_move_bytecode_verifier_impl(
                    &modules,
                    &protocol_config,
                    &metered_verifier_config,
                    &mut meter,
                    &bytecode_verifier_metrics,
                );
                let elapsed = start.elapsed();
                let size = package.size();
                let num_modules = modules.len();
                let fun_meter = meter.get_usage(Scope::Function);
                let mod_meter = meter.get_usage(Scope::Module);

                let text = format!(
                    "{}, {}, {}, {}, {}, {} {:?} \n",
                    package.id(),
                    size,
                    elapsed.as_micros(),
                    fun_meter,
                    mod_meter,
                    num_modules,
                    status
                );

                f.write_all(text.as_bytes()).expect("Unable to write data");
            }
            _ => {}
        }
    }
}

pub fn try_construct_package_object(
    object_key: &ObjectKey,
    store_object_wrapper: &StoreObjectWrapper,
) -> Option<Object> {
    let store_object = match store_object_wrapper {
        StoreObjectWrapper::V1(store_object) => store_object,
        _ => return None,
    };
    let store_object = match store_object {
        StoreObjectV1::Value(store_object) => store_object,
        _ => return None,
    };
    match &store_object.data {
        StoreData::Package(package)
            if BuiltInFramework::all_package_ids().contains(&package.id()) =>
        {
            None
        }
        StoreData::Package(package) => Some(Object {
            data: Data::Package(package.clone()),
            owner: store_object.owner,
            previous_transaction: store_object.previous_transaction,
            storage_rebate: store_object.storage_rebate,
        }),
        _ => None,
    }
}
