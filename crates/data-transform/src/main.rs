use data_transform::*;
use diesel::prelude::*;
use diesel::RunQueryDsl;
use diesel::QueryableByName;
use diesel::pg::sql_types::Bytea;
use anyhow::anyhow;
use std::sync::Arc;
use std::process::exit;

use sui_types::object::MoveObject;
use sui_types::object::ObjectFormatOptions;
use move_bytecode_utils::module_cache::SyncModuleCache;
use move_core_types::value::MoveStruct;

use sui_indexer::new_pg_connection_pool;
use self::models::*;
use std::env;
use sui_indexer::store::module_resolver::IndexerModuleResolver;
use sui_indexer::errors::IndexerError;

use sui_types::parse_sui_struct_tag;
use sui_json_rpc_types::SuiMoveStruct;
use move_core_types::language_storage::ModuleId;
use move_bytecode_utils::module_cache::GetModule;

use tracing::debug;

fn main() {
    #[derive(QueryableByName)]
    #[derive(Debug)]
    struct ModuleBytes {
        #[diesel(sql_type = Bytea)]
        data: Vec<u8>,
    }

    use self::schema::events::dsl::*;
    use self::schema::events_json::dsl::*;

    // get the starting id from the arguments
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        eprintln!("Usage: data-transform <id>");
        exit(0);
    }

    let start_id: i64 = match args[1].parse() {
        Ok(num) => num,
        Err(_) => {
            eprintln!("Invalid integer: {}", args[1]);
            exit(0);
        }
    };

    println!("start id = {}", start_id);

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let connection = &mut establish_connection();

    let blocking_cp = new_pg_connection_pool(&database_url).map_err(|e| anyhow!("Unable to connect to Postgres, is it running? {e}"));
    let module_cache = Arc::new(SyncModuleCache::new(IndexerModuleResolver::new(blocking_cp.expect("REASON").clone())));

    for target_id in start_id.. {

        let event = events
            .find(target_id)
            .select(Event::as_select())
            .first(connection)
            .optional();

        match event {
            Ok(Some(event)) => {
                println!("-----------\n");
                println!("event id = {}", event.id);
                debug!("event sequence = {:#?}", event.event_sequence);
                debug!("sender = {:#?}", event.sender);
                println!("package = {:#?}", event.package);
                debug!("module = {:#?}", event.module);
                debug!("type = {:#?}", event.event_type);
                let text = String::from_utf8_lossy(&event.event_bcs);
                debug!("bcs in text = {:#?}", text);

                if event.package != "0x000000000000000000000000000000000000000000000000000000000000dee9" {
                    println!("not deepbook skipping...");
                    continue;
                }

                // check for the previous record in events_json
                let eventj = events_json
                    .find(target_id)
                    .select(EventsJson::as_select())
                    .first(connection)
                    .optional();

                match eventj {
                    Ok(Some(_eventj)) => {
                        println!("Already processed {}, skipping...", target_id);
                        continue;
                    }
                    Ok(None) => {
                        println!("Unable to find event_json {}", target_id);
                    }
                    Err(_) => {
                        println!("An error occured while fetching event_json {}", target_id);
                    }
                }


                // JSON parsing starts here
                let type_ = parse_sui_struct_tag(&event.event_type).expect("cannot load StructTag");
                let module_id = ModuleId::new(type_.address, type_.module.clone());
                println!("module id = {}", module_id);

                let newmodule = module_cache.get_module_by_id(&module_id).expect("Module {module_id} must load").unwrap();
                println!("{newmodule:#?}");

                println!("iterating...");
                for type_def in &newmodule.struct_defs {
                    println!("- {:#?}", newmodule.struct_handles[type_def.struct_handle.0 as usize]);
                    let handle = &newmodule.struct_handles[type_def.struct_handle.0 as usize];
                    let name_idx = handle.name;
                    println!("struct {:?}", newmodule.identifiers[name_idx.0 as usize]);
                }

                let layout = MoveObject::get_layout_from_struct_tag(
                    type_,
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
                                println!("event json = {}", final_result);

                                let new_event_json = EventsJson { id: event.id, event_json: final_result };

                                let _inserted_event_json = diesel::insert_into(events_json)
                                    .values(&new_event_json)
                                    .execute(connection)
                                    .expect("Error saving new events json");

                                println!("Inserted new event_json id: {}", event.id);

                            }|
                            Err(e) => {
                                println!("error in deserialize:{}", e);
                                exit(0);
                            }
                        }
                    }
                    Err(err) => {
                        println!("error in get_layout: {}", err);
                        exit(0);
                    }
                }
            }
            Ok(None) => {
                println!("Unable to find event {}", target_id);
                exit(0);
            }
            Err(_) => {
                println!("An error occured while fetching event {}", target_id);
                exit(0);
            }
        }
    }
}
