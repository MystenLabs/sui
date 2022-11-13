// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#[macro_use]
extern crate rocket;

use rocket::data::{Data, ToByteUnit};
use rocket::http::uri::Absolute;
use rocket::response::content::RawText;
use rocket::tokio::fs::{self, File};
use std::env;
use std::path::Path;


use sui_config::genesis::Genesis;


#[get("/<object_id>")]
async fn object_id(object_id: String) -> String {
    return format!("{:?}", env::current_dir());
    let genesis = Genesis::load("genesis.blob");
    format!("{:#?}", genesis)
}

// TODO
// 1. dump genesis
// 2. get current committee info given genesis blob
// 3. get object past version, locked, if equivocated
// 4. get all equivocated object info
// 5. get 

#[get("/<_..>", rank = 2)]
fn index() -> &'static str {
    "
    USAGE
      POST /
          accepts raw data in the body of the request and responds with a URL of
          a page containing the body's content
          EXAMPLE: curl --data-binary @file.txt http://localhost:8000
      GET /<id>
          retrieves the content for the paste with id `<id>`
    "
}

#[launch]
fn rocket() -> _ {
    rocket::build()
    .mount("/", routes![index])
    .mount("/object_id", routes![object_id])
}
