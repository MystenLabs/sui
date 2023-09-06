#[macro_use]
extern crate diesel;

use diesel::prelude::*;
use diesel::r2d2::{self, ConnectionManager};
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

    let manager = ConnectionManager::<MysqlConnection>::new(database_url);
    let pool: r2d2::Pool<ConnectionManager<MysqlConnection>> = r2d2::Pool::builder()
        .build(manager)
        .expect("Failed to create pool.");

    // Use a connection
    let connection = pool.get().expect("Failed to get a connection from the pool");

    let new_post = NewPost { title: "My Title 234", body: "My Body 123" };

    diesel::insert_into(posts::table)
        .values(&new_post)
        .execute(&connection)
        .expect("Error inserting new post");
}
