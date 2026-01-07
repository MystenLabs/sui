// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub mod objects;
pub mod read;
pub mod write;

use std::collections::BTreeMap;

use anyhow::Context as _;
use tracing::info;

use sui_indexer_alt_jsonrpc::RpcService;
use sui_indexer_alt_jsonrpc::api::coin::Coins;
use sui_indexer_alt_jsonrpc::api::governance::Governance;
use sui_indexer_alt_metrics::MetricsService;

use crate::context::Context;
use crate::rpc::objects::insert_package_into_db;
use crate::store::ForkingStore;
use std::slice::from_ref;
use sui_data_store::ObjectKey;
use sui_data_store::ObjectStore;
use sui_data_store::VersionQuery;
use sui_indexer_alt_jsonrpc::api::checkpoints::Checkpoints;
use sui_indexer_alt_jsonrpc::api::transactions::QueryTransactions;
use sui_indexer_alt_jsonrpc::api::transactions::Transactions;
use sui_types::base_types::ObjectID;
use sui_types::object::Object;

pub(crate) async fn fetch_and_cache_object_from_rpc(
    data_store: &mut ForkingStore,
    context: &Context,
    object_id: &ObjectID,
) -> Result<Object, anyhow::Error> {
    let Context {
        at_checkpoint,
        db_writer,
        ..
    } = context;

    let obj = data_store.get_object(object_id);

    if let Some(obj) = obj {
        Ok(obj.clone())
    } else {
        info!("Object not found locally: {:?}", object_id);

        let obj = data_store
            .get_rpc_data_store()
            .get_objects(&[ObjectKey {
                object_id: *object_id,
                version_query: VersionQuery::AtCheckpoint(*at_checkpoint),
            }])
            .unwrap();
        let obj = obj.into_iter().next().unwrap();

        if let Some((ref object, _version)) = obj {
            info!("Fetched object from rpc: {:?}", object.id());
            let written_objects = BTreeMap::from([(*object_id, object.clone())]);
            data_store.update_objects(written_objects, vec![]);

            // If this is a package, insert it into kv_packages table
            if object.is_package()
                && let Err(e) =
                    insert_package_into_db(db_writer, from_ref(object), *at_checkpoint).await
            {
                eprintln!("Failed to insert package into DB: {:?}", e);
            }

            Ok(object.clone())
        } else {
            anyhow::bail!("Object {:?} not found in RPC store", object_id);
        }
    }
}

pub(crate) async fn start_rpc(
    context: Context,
    mut rpc: RpcService,
    metrics: MetricsService,
) -> anyhow::Result<()> {
    // indexer-alt-jsonrpc defined modules
    rpc.add_module(Checkpoints(context.pg_context.clone()))?;
    rpc.add_module(Coins(context.pg_context.clone()))?;
    rpc.add_module(Governance(context.pg_context.clone()))?;
    rpc.add_module(Transactions(context.pg_context.clone()))?;
    rpc.add_module(QueryTransactions(context.pg_context.clone()))?;

    // Local RPC defined modules
    rpc.add_module(objects::Objects(context.clone()))?;
    rpc.add_module(objects::QueryObjects(context.clone()))?;
    rpc.add_module(read::Read(context.clone()))?;
    rpc.add_module(write::Write(context.clone()))?;

    // run services
    let s_metrics = metrics.run().await?;
    let h_rpc = rpc.run().await.context("Failed to start RPC service")?;

    match h_rpc.attach(s_metrics).main().await {
        Ok(()) | Err(sui_futures::service::Error::Terminated) => {}

        Err(sui_futures::service::Error::Aborted) => {
            std::process::exit(1);
        }

        Err(sui_futures::service::Error::Task(_)) => {
            std::process::exit(2);
        }
    }
    Ok(())
}
