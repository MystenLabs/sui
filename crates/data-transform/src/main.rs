use data_transform::*;
use diesel::prelude::*;
use diesel::RunQueryDsl;
use diesel::QueryableByName;
use diesel::sql_query;
use diesel::sql_types::Text;
use diesel::sql_types::Integer;
use diesel::pg::sql_types::Bytea;
use sui_types::base_types::ObjectID;
use move_core_types::language_storage::ModuleId;
use anyhow::anyhow;
use std::sync::Arc;

use sui_types::object::MoveObject;
use sui_types::object::ObjectFormatOptions;
use move_bytecode_utils::module_cache::GetModule;
use move_bytecode_utils::module_cache::SyncModuleCache;
use move_core_types::value::{MoveFieldLayout, MoveStruct, MoveStructLayout, MoveTypeLayout};

use sui_indexer::{get_pg_pool_connection, new_pg_connection_pool, Indexer, IndexerConfig};
use self::models::*;
use std::env;
use sui_indexer::store::module_resolver::IndexerModuleResolver;
use sui_indexer::errors::IndexerError;

use sui_types::parse_sui_struct_tag;
use sui_json_rpc_types::{SuiEvent, SuiMoveStruct};
use serde::Serialize;

use std::process::{Command, Stdio};
use std::io::{self, Read};

const LATEST_MODULE_QUERY: &str = "SELECT (t2.module).data
FROM (SELECT UNNEST(data) AS module
            FROM (SELECT data FROM packages WHERE package_id = $1 ORDER BY version DESC FETCH FIRST 1 ROW ONLY) t1) t2
WHERE (module).name = $2;";

fn main() {
    #[derive(QueryableByName)]
    #[derive(Debug)]
    struct ModuleBytes {
        #[diesel(sql_type = Bytea)]
        data: Vec<u8>,
    }

    use self::schema::events::dsl::*;

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let connection = &mut establish_connection();

    let start_id = 1;
    let max_id = 5;

    let blocking_cp = new_pg_connection_pool(&database_url).map_err(|e| anyhow!("Unable to connect to Postgres, is it running? {e}"));
    let module_cache = Arc::new(SyncModuleCache::new(IndexerModuleResolver::new(blocking_cp.expect("REASON").clone())));

    for target_id in start_id..=max_id {

        let event = events
            .find(target_id)
            .select(Event::as_select())
            .first(connection)
            .optional();

        match event {
            Ok(Some(event)) => {
                println!("-----------\n");
                println!("event id = {}", event.id);
                println!("event sequence = {:#?}", event.event_sequence);
                println!("sender = {:#?}", event.sender);
                println!("package = {:#?}", event.package);
                println!("module = {:#?}", event.module);
                println!("type = {:#?}", event.event_type);
                let text = String::from_utf8_lossy(&event.event_bcs);
                println!("bcs in text = {:#?}", text);

                // JSON parsing starts here
                let type_ = parse_sui_struct_tag(&event.event_type);

                let package_id = event.package.to_string();
                println!("package id= {:#?}", package_id);

                let module_name = event.module.to_string();
                println!("module name= {:#?}", module_name);

                /*
                let result = diesel::sql_query(LATEST_MODULE_QUERY)
                    .bind::<diesel::sql_types::Text, _>(package_id)
                    .bind::<diesel::sql_types::Text, _>(module_name)
                    .get_result::<ModuleBytes>(connection);

                println!("{:?}", result);
                */

                let layout = MoveObject::get_layout_from_struct_tag(
                    type_.expect("REASON").clone(),
                    ObjectFormatOptions::default(),
                    &module_cache,
                    );

                match layout {
                    Ok(l) => {
                        let move_object = MoveStruct::simple_deserialize(&event.event_bcs, &l)
                            .map_err(|e| IndexerError::SerdeError(e.to_string()));

                        match move_object {
                            Ok(m) => {
                                let parsed_json = SuiMoveStruct::from(m).to_json_value();
                                let final_result = serde_json::to_string_pretty(&parsed_json).unwrap();
                                println!("final = {}", final_result);
                            }|
                            Err(e) => {
                                println!("error: {}", e);
                                continue;
                            }
                        }
                    }
                    Err(err) => {
                        println!("error: {}", err);
                        continue;
                    }
                }
            }
            Ok(None) => {
                println!("Unable to find event {}", target_id);
                continue;
            }
            Err(_) => {
                println!("An error occured while fetching event {}", target_id);
                continue;
            }
        }
    }
}
