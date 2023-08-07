use data_transform::*;
use diesel::prelude::*;
use diesel::RunQueryDsl;

use self::models::*;

use sui_types::parse_sui_struct_tag;

fn main() {

    use self::schema::events::dsl::*;

    let connection = &mut establish_connection();

    let max_id = 5;

    for target_id in 1..=max_id {
        println!("{}", target_id);

        let event = events
            .find(target_id)
            .select(Event::as_select())
            .first(connection)
            .optional();

        match event {
            Ok(Some(event)) => {
                println!("event id = {}", event.id);
                println!("event sequence = {:#?}", event.event_sequence);
                println!("sender = {:#?}", event.sender);
                println!("package = {:#?}", event.package);
                println!("module = {:#?}", event.module);
                println!("type = {:#?}", event.event_type);
                let text = String::from_utf8_lossy(&event.event_bcs);
                println!("bcs in text = {:#?}", text);
                println!("-----------\n");

                // JSON parsing starts here
                // Get the type
                let type_ = parse_sui_struct_tag(&event.event_type);
                dbg!(type_);
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
