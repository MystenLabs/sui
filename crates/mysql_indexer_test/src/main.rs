#[macro_use]
extern crate diesel;

use diesel::prelude::*;
use diesel::mysql::MysqlConnection;
use dotenv::dotenv;
use std::env;

table! {
    posts (id) {
        id -> Integer,
        title -> Varchar,
        body -> Text,
    }
}

#[derive(Insertable)]
#[table_name = "posts"]
struct NewPost<'a> {
    title: &'a str,
    body: &'a str,
}

fn main() {
    dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let connection = MysqlConnection::establish(&database_url).expect("Error connecting to database");

    let new_post = NewPost { title: "My Title", body: "My Body" };

    diesel::insert_into(posts::table)
        .values(&new_post)
        .execute(&connection)
        .expect("Error inserting new post");
}
