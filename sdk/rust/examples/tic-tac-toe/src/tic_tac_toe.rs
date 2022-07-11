// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use clap::Parser;
use clap::Subcommand;
use serde::Deserialize;
use std::path::PathBuf;
use std::str::FromStr;
use std::thread;
use std::time::Duration;
use sui_client::apis::{
    RpcBcsApi, RpcGatewayApi, RpcReadApi, RpcTransactionBuilder, WalletSyncApi,
};
use sui_client::keystore::{Keystore, SuiKeystore};
use sui_client::SuiRpcClient;
use sui_json::SuiJsonValue;
use sui_json_rpc_types::GetObjectDataResponse;

use sui_types::base_types::{ObjectID, SuiAddress};
use sui_types::id::VersionedID;
use sui_types::sui_serde::Base64;

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opts: TicTacToeOpts = TicTacToeOpts::parse();
    let keystore_path = opts.keystore_path.unwrap_or_else(default_keystore_path);
    let keystore = SuiKeystore::load_or_create(&keystore_path)?;

    let game = TicTacToe {
        game_package_id: opts.game_package_id,
        client: SuiRpcClient::new_http_client(&opts.rpc_server_url)?,
        keystore,
    };

    match opts.subcommand {
        TicTacToeCommand::NewGame { player_x, player_o } => {
            game.create_game(player_x, player_o).await?;
        }
        TicTacToeCommand::JoinGame {
            my_identity,
            game_id,
        } => {
            game.join_game(game_id, my_identity).await?;
        }
    }

    Ok(())
}

struct TicTacToe {
    game_package_id: ObjectID,
    client: SuiRpcClient,
    keystore: SuiKeystore,
}

impl TicTacToe {
    async fn create_game(
        &self,
        player_x: Option<SuiAddress>,
        player_o: Option<SuiAddress>,
    ) -> Result<(), anyhow::Error> {
        let player_x = player_x.unwrap_or_else(|| self.keystore.addresses()[0]);
        let player_o = player_o.unwrap_or_else(|| self.keystore.addresses()[1]);

        self.client.sync_account_state(player_x).await?;

        // Create a move call transaction using the TransactionBuilder API.
        let create_game_call = self
            .client
            .move_call(
                player_x,
                self.game_package_id,
                "shared_tic_tac_toe".to_string(),
                "create_game".to_string(),
                vec![],
                vec![
                    SuiJsonValue::from_str(&player_x.to_string())?,
                    SuiJsonValue::from_str(&player_o.to_string())?,
                ],
                None,
                1000,
            )
            .await?;

        // Sign the transaction.
        let transaction_bytes = create_game_call.tx_bytes.to_vec()?;
        let signature = self.keystore.sign(&player_x, &transaction_bytes)?;
        let signature_base64 = Base64::from_bytes(signature.signature_bytes());
        let pub_key = Base64::from_bytes(signature.public_key_bytes());

        // Execute the transaction.
        let response = self
            .client
            .execute_transaction(create_game_call.tx_bytes, signature_base64, pub_key)
            .await?;

        // We know `create_game` move function will create 1 object.
        let game_id = response
            .to_effect_response()?
            .effects
            .created
            .first()
            .unwrap()
            .reference
            .object_id;

        println!("Create new game [{}]", game_id);
        println!("Player X : {}", player_x);
        println!("Player O : {}", player_o);

        self.join_game(game_id, player_x).await?;
        Ok(())
    }

    async fn join_game(
        &self,
        game_id: ObjectID,
        my_identity: SuiAddress,
    ) -> Result<(), anyhow::Error> {
        let current_game = self.client.get_raw_object(game_id).await?;
        let current_game_bytes = current_game
            .object()?
            .data
            .try_as_move()
            .map(|m| &m.bcs_bytes)
            .unwrap();

        let game_state: TicTacToeState = bcs::from_bytes(current_game_bytes)?;
        if game_state.o_address == my_identity {
            println!("You are player O")
        } else if game_state.x_address == my_identity {
            println!("You are player X")
        } else {
            return Err(anyhow!("You are not invited to the game."));
        }

        let current_player = if game_state.cur_turn % 2 == 0 {
            game_state.x_address
        } else {
            game_state.o_address
        };

        if current_player == my_identity {
            println!("It's your turn!");
        } else {
            println!("Waiting for opponent...");
        }

        loop {
            let response: GetObjectDataResponse = self.client.get_object(game_id).await?;
            let o = response.object()?;
            println!("{}", o);
            thread::sleep(Duration::from_secs(5));
        }
    }
}

// Clap command line args parser
#[derive(Parser)]
#[clap(
    name = "tic-tac-toe",
    about = "A Byzantine fault tolerant Tic-Tac-Toe with low-latency finality and high throughput",
    rename_all = "kebab-case"
)]
struct TicTacToeOpts {
    game_package_id: ObjectID,
    keystore_path: Option<PathBuf>,
    #[clap(default_value = "http://127.0.0.1:5001")]
    rpc_server_url: String,
    #[clap(subcommand)]
    subcommand: TicTacToeCommand,
}

fn default_keystore_path() -> PathBuf {
    match dirs::home_dir() {
        Some(v) => v.join(".sui").join("sui_config").join("sui.keystore"),
        None => panic!("Cannot obtain home directory path"),
    }
}

#[derive(Subcommand)]
#[clap(rename_all = "kebab-case")]
enum TicTacToeCommand {
    NewGame {
        player_x: Option<SuiAddress>,
        player_o: Option<SuiAddress>,
    },
    JoinGame {
        my_identity: SuiAddress,
        game_id: ObjectID,
    },
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct TicTacToeState {
    id: VersionedID,
    gameboard: Vec<Vec<u8>>,
    cur_turn: u8,
    game_status: u8,
    x_address: SuiAddress,
    o_address: SuiAddress,
}
