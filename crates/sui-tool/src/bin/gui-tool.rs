// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! GUI is a web tool application for Sui network. Example:
//! ```
//! FAUCET_URL=https://faucet.devnet.sui.io \
//! GENESIS_PATH=genesis.blob \
//! ENV=devnet \
//! cargo run --bin gui-tool
//! ```
//! You need to have templates folder in the working directory where you
//! launch the program. For example, you could:
//! ```
//! ln -s crates/sui-tool/src/bin/templates/ templates
//! ```
//! After launch, open http://127.0.0.1:8000 with your browser

#![allow(clippy::unnecessary_lazy_evaluations)]

#[macro_use]
extern crate rocket;

use futures::FutureExt;
use rocket::form::Form;
use rocket::http::Status;
use rocket::response::status::Custom;
use rocket::Request;
use rocket::State;
use rocket_dyn_templates::{context, Template};
use serde::Serialize;
use serde_with::serde_as;
use std::env;
use std::panic::AssertUnwindSafe;
use std::path::PathBuf;
use std::str::FromStr;
use sui_cluster_test::config::ClusterTestOpt;
use sui_cluster_test::faucet::{FaucetClient, RemoteFaucetClient};
use sui_cluster_test::ClusterTest;
use sui_config::genesis::Genesis;
use sui_config::ValidatorInfo;
use sui_tool::get_transaction;
use sui_tool::{get_object, GrouppedObjectOutput};
use sui_types::base_types::TransactionDigest;
use sui_types::base_types::{ObjectID, SuiAddress};
use telemetry_subscribers::FilterHandle;
use telemetry_subscribers::TelemetryGuards;
use tokio::sync::Mutex;
use tracing::info;

const CLUSTER_TEST_STR: &str = "cluster_test";

#[get("/")]
async fn cluster_test_home(config: &State<Config>) -> Template {
    info!("cluster_test_home");
    let env = &config.cluster_test_opt.env.to_string();
    Template::render(
        "cluster-test-home",
        context![title: "Cluste Test", network: env],
    )
}

#[post("/")]
async fn cluster_test_run(config: &State<Config>) -> Template {
    info!("cluster_test_run");
    let opt = config.cluster_test_opt.clone();

    let future = ClusterTest::run(opt);

    if let Err(err) = AssertUnwindSafe(future).catch_unwind().await {
        let panic_str = match err.downcast::<String>() {
            Ok(v) => *v,
            Err(_) => "Panic info cannot be stringified.".to_owned(),
        };

        return Template::render(
            "error",
            context! {
                title: "Cluster Test Run Failed!",
                error: format!(r#"{:#?}"#, panic_str),
            },
        );
    }
    Template::render(
        "cluster-test",
        context! {
            title: "Cluster Test",
            response: "All passed!",
        },
    )
}

#[catch(500)]
fn internal_server_error(req: &Request) -> Template {
    info!(?req, "internal_server_error");
    Template::render(
        "error",
        context! {
            title: "",
            error: "Oops, some internal server error occurred.".to_owned(),
        },
    )
}

#[get("/")]
async fn faucet_home() -> Template {
    info!("faucet_home");
    Template::render("faucet-home", context![title: "Faucet"])
}

#[post("/", data = "<address>")]
async fn faucet_request(address: Form<String>, config: &State<Config>) -> Template {
    info!(?address, "faucet_submit");
    let address = address.into_inner();

    let address = ObjectID::from_hex_literal(&address)
        .map_err(|_e| Custom(Status::BadRequest, "Invalid hex address."));
    let address = return_if_error!(address);

    let faucet_url = config.faucet_url.clone();

    let faucet_client = RemoteFaucetClient::new(faucet_url);
    let faucet_response = faucet_client
        .request_sui_coins(SuiAddress::from(address))
        .await;
    Template::render(
        "faucet",
        context! {
            title: "Faucet",
            response: format!(r#"{:#?}"#, faucet_response)
        },
    )
}

#[get("/")]
fn validators(config: &State<Config>) -> Template {
    info!("validators");
    let genesis_path = config.genesis_path.clone();
    let genesis = Genesis::load(genesis_path).map_err(|err| {
        Custom(
            Status::InternalServerError,
            format!("Can't load genesis.blob, err: {:?}", err),
        )
    });
    let genesis = return_if_error!(genesis);
    let validators = genesis
        .validator_set()
        .iter()
        .map(|v| ValidatorInfoForDisplay {
            inner: v.clone(),
            account_key_hex: hex::encode(v.account_key()),
            protocol_key_hex: hex::encode(v.protocol_key()),
            worker_key_hex: hex::encode(v.worker_key()),
            network_key_hex: hex::encode(v.network_key()),
        })
        .collect::<Vec<_>>();
    Template::render(
        "validators",
        context! {
            title: "Validators",
            validators: &validators,
        },
    )
}

#[get("/")]
async fn object_home() -> Template {
    info!("object_home");
    Template::render("object-home", context![title: "Object"])
}

#[get("/query?<object_id>&<object_version>")]
async fn object_query(
    object_id: String,
    object_version: String,
    config: &State<Config>,
) -> Template {
    info!(object_id, object_version, "object_query");

    let object_id = ObjectID::from_hex_literal(&object_id)
        .map_err(|_e| Custom(Status::BadRequest, "Invalid hex address."));
    let object_id = return_if_error!(object_id);
    let object_version = if object_version.is_empty() {
        None
    } else {
        Some(return_if_error!(object_version.parse::<u64>()))
    };

    let data = get_object(
        object_id,
        object_version,
        None,
        PathBuf::from(config.genesis_path.clone()),
        false,
    )
    .await;
    let data = return_if_error!(data);
    let response = format!(r#"{}"#, GrouppedObjectOutput(data));

    Template::render(
        "object",
        context! {
            title: format!("Object: {:?}", object_id),
            response,
        },
    )
}

#[get("/")]
async fn tx_home() -> Template {
    info!("tx_home");
    Template::render("transaction-home", context![title: "Transaction"])
}

#[get("/query?<tx_digest>")]
async fn tx_query(tx_digest: String, config: &State<Config>) -> Template {
    let tx_digest = TransactionDigest::from_str(&tx_digest)
        .map_err(|_e| Custom(Status::BadRequest, "Invalid transaction digest."));

    let tx_digest = return_if_error!(tx_digest);

    let data = get_transaction(tx_digest, PathBuf::from(config.genesis_path.clone())).await;
    let data = return_if_error!(data);

    Template::render(
        "transaction",
        context! {
            title: format!("Transaction: {:?}", tx_digest),
            response: data
        },
    )
}

#[get("/")]
fn index() -> Template {
    info!("index");
    Template::render(
        "index",
        context! {
            title: "Home",
        },
    )
}

struct Config {
    genesis_path: String,
    faucet_url: String,
    cluster_test_opt: ClusterTestOpt,
    _guard: Mutex<(TelemetryGuards, FilterHandle)>,
}

impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "genesis_path: {}", self.genesis_path)?;
        writeln!(f, "faucet_url: {}", self.faucet_url)?;
        write!(f, "cluster_test_opt: {:?}", self.cluster_test_opt)
    }
}

#[launch]
fn rocket() -> _ {
    let _guard = Mutex::new(
        telemetry_subscribers::TelemetryConfig::new(env!("CARGO_BIN_NAME"))
            .with_env()
            .init(),
    );
    let env = env::var("ENV").expect("Env var $ENV is set");
    let opt = ClusterTestOpt::try_from(&env).expect("Expect valid $ENV");
    let config = Config {
        genesis_path: env::var("GENESIS_PATH").expect("Env var $GENESIS_PATH is set"),
        faucet_url: env::var("FAUCET_URL").expect("Env var $FAUCET_URL is set"),
        cluster_test_opt: opt,
        _guard,
    };
    info!(?config, "Rocket Server Starts.");
    rocket::build()
        .manage(config)
        .mount("/", routes![index])
        .mount("/validators", routes![validators])
        .mount("/faucet", routes![faucet_request, faucet_home])
        .mount("/object", routes![object_query, object_home])
        .mount("/transaction", routes![tx_query, tx_home])
        .mount(
            format!("/{}", CLUSTER_TEST_STR),
            routes![cluster_test_home, cluster_test_run],
        )
        .register("/", catchers![internal_server_error])
        .attach(Template::fairing())
}

#[serde_as]
#[derive(Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct ValidatorInfoForDisplay {
    pub inner: ValidatorInfo,
    pub account_key_hex: String,
    pub protocol_key_hex: String,
    pub worker_key_hex: String,
    pub network_key_hex: String,
}

#[macro_export]
macro_rules! return_if_error {
    ($val:expr) => {
        if let Err(e) = $val {
            return Template::render(
                "error",
                context! {
                    title: "",
                    error: format!(r#"{:#?}"#, e)
                },
            );
        } else {
            $val.unwrap()
        }
    };
}
