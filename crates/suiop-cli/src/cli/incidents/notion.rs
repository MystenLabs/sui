use notion::models::search::DatabaseQuery;
use notion::models::Page;
use notion::NotionApi;
use std::env;
use std::str::FromStr;

pub struct Notion {
    client: NotionApi,
}

impl Notion {
    pub fn new() -> Self {
        let token = env::var("NOTION_API_TOKEN")
            .expect("Please set the NOTION_API_TOKEN environment variable");
        let client = NotionApi::new(token).expect("Failed to create Notion API client");
        Self { client }
    }

    pub async fn get_incident_selection_incidents(self) {
        // let db_id = notion::ids::DatabaseId::from_str("7be81ba2838045cab4ad6b7326ca6622")
        let db_id = notion::ids::DatabaseId::from_str("a8da55dadb524e7db202b4dfd799d9ce")
            .expect("Invalid Database ID");

        // Retrieve the db
        match self
            .client
            .query_database(db_id, DatabaseQuery::default())
            .await
        {
            Ok(results) => {
                println!("Database query results: {:#?}", results);
            }
            Err(err) => {
                eprintln!("Error querying database: {:?}", err);
            }
        }
    }
}
