// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::anyhow;
use async_recursion::async_recursion;
use clap::Parser;
use clap::Subcommand;
use serde::Deserialize;
use std::io::{stdin, stdout, Write};
use std::path::PathBuf;
use std::str::FromStr;
use std::thread;
use std::time::Duration;
use sui_sdk::crypto::KeystoreType;
use sui_sdk::{
    crypto::SuiKeystore,
    json::SuiJsonValue,
    types::{
        base_types::{ObjectID, SuiAddress},
        id::UID,
        messages::Transaction,
    },
    SuiClient,
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    let opts: TicTacToeOpts = TicTacToeOpts::parse();
    let keystore_path = opts.keystore_path.unwrap_or_else(default_keystore_path);
    let keystore = KeystoreType::File(keystore_path).init()?;

    let game = TicTacToe {
        game_package_id: opts.game_package_id,
        client: SuiClient::new_http_client(&opts.rpc_server_url)?,
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
    client: SuiClient,
    keystore: SuiKeystore,
}

impl TicTacToe {
    async fn create_game(
        &self,
        player_x: Option<SuiAddress>,
        player_o: Option<SuiAddress>,
    ) -> Result<(), anyhow::Error> {
        // Default player identity to first and second keys in the keystore if not provided.
        let player_x = player_x.unwrap_or_else(|| self.keystore.addresses()[0]);
        let player_o = player_o.unwrap_or_else(|| self.keystore.addresses()[1]);

        // Force a sync of signer's state in gateway.
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
                None, // The gateway server will pick a gas object belong to the signer if not provided.
                1000,
            )
            .await?;

        // Get signer from keystore
        let signer = self.keystore.signer(player_x);

        // Sign the transaction
        let tx = Transaction::from_data(create_game_call, &signer);

        // Execute the transaction.
        let response = self.client.execute_transaction(tx).await?;

        // We know `create_game` move function will create 1 object.
        let game_id = response
            .effects
            .created
            .first()
            .unwrap()
            .reference
            .object_id;

        println!("Created new game, game id : [{}]", game_id);
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
        let game_state = self.fetch_game_state(game_id).await?;
        if game_state.o_address == my_identity {
            println!("You are player O")
        } else if game_state.x_address == my_identity {
            println!("You are player X")
        } else {
            return Err(anyhow!("You are not invited to the game."));
        }
        self.next_turn(my_identity, game_state).await
    }

    #[async_recursion]
    async fn next_turn(
        &self,
        my_identity: SuiAddress,
        game_state: TicTacToeState,
    ) -> Result<(), anyhow::Error> {
        game_state.print_game_board();

        // return if game ended.
        if game_state.game_status != 0 {
            println!("Game ended.");
            match game_state.game_status {
                1 => println!("Player X won!"),
                2 => println!("Player O won!"),
                3 => println!("It's a draw!"),
                _ => {}
            }
            return Ok(());
        }

        if game_state.is_my_turn(my_identity) {
            println!("It's your turn!");
            let row = get_row_col_input(true) - 1;
            let col = get_row_col_input(false) - 1;

            // Create a move call transaction using the TransactionBuilder API.
            let place_mark_call = self
                .client
                .move_call(
                    my_identity,
                    self.game_package_id,
                    "shared_tic_tac_toe".to_string(),
                    "place_mark".to_string(),
                    vec![],
                    vec![
                        SuiJsonValue::from_str(&game_state.info.object_id().to_hex_literal())?,
                        SuiJsonValue::from_str(&row.to_string())?,
                        SuiJsonValue::from_str(&col.to_string())?,
                    ],
                    None,
                    1000,
                )
                .await?;

            // Get signer from keystore
            let signer = self.keystore.signer(my_identity);

            let transaction = Transaction::from_data(place_mark_call, &signer);
            // Execute the transaction.
            let response = self.client.execute_transaction(transaction).await?;

            // Print any execution error.
            let status = response.effects.status;
            if status.is_err() {
                eprintln!("{:?}", status);
            }
            // Proceed to next turn.
            self.next_turn(
                my_identity,
                self.fetch_game_state(*game_state.info.object_id()).await?,
            )
            .await?;
        } else {
            println!("Waiting for opponent...");
            // Sleep until my turn.
            while !self
                .fetch_game_state(*game_state.info.object_id())
                .await?
                .is_my_turn(my_identity)
            {
                thread::sleep(Duration::from_secs(1));
            }
            self.next_turn(
                my_identity,
                self.fetch_game_state(*game_state.info.object_id()).await?,
            )
            .await?;
        };
        Ok(())
    }

    // Retrieve the latest game state from the server.
    async fn fetch_game_state(&self, game_id: ObjectID) -> Result<TicTacToeState, anyhow::Error> {
        // Get the raw BCS serialised move object data
        let current_game = self.client.get_raw_object(game_id).await?;
        let current_game_bytes = current_game
            .object()?
            .data
            .try_as_move()
            .map(|m| &m.bcs_bytes)
            .unwrap();
        // Deserialize the data bytes into TicTacToeState struct
        Ok(bcs::from_bytes(current_game_bytes)?)
    }
}

// Helper function for getting console input
fn get_row_col_input(is_row: bool) -> u8 {
    let r_c = if is_row { "row" } else { "column" };
    print!("Enter {} number (1-3) : ", r_c);
    let _ = stdout().flush();
    let mut s = String::new();
    stdin()
        .read_line(&mut s)
        .expect("Did not enter a correct string");

    if let Ok(number) = s.trim().parse() {
        if number > 0 && number < 4 {
            return number;
        }
    }
    get_row_col_input(is_row)
}

// Clap command line args parser
#[derive(Parser)]
#[clap(
    name = "tic-tac-toe",
    about = "A Byzantine fault tolerant Tic-Tac-Toe with low-latency finality and high throughput",
    rename_all = "kebab-case"
)]
struct TicTacToeOpts {
    #[clap(long)]
    game_package_id: ObjectID,
    #[clap(long)]
    keystore_path: Option<PathBuf>,
    #[clap(long, default_value = "https://gateway.devnet.sui.io:443")]
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
        #[clap(long)]
        player_x: Option<SuiAddress>,
        #[clap(long)]
        player_o: Option<SuiAddress>,
    },
    JoinGame {
        #[clap(long)]
        my_identity: SuiAddress,
        #[clap(long)]
        game_id: ObjectID,
    },
}

// Data structure mirroring move object `games::shared_tic_tac_toe::TicTacToe` for deserialization.
#[derive(Deserialize, Debug)]
struct TicTacToeState {
    info: UID,
    gameboard: Vec<Vec<u8>>,
    cur_turn: u8,
    game_status: u8,
    x_address: SuiAddress,
    o_address: SuiAddress,
}

impl TicTacToeState {
    fn print_game_board(&self) {
        println!("     1     2     3");
        print!("  ┌-----┬-----┬-----┐");
        let mut row_num = 1;
        for row in &self.gameboard {
            println!();
            print!("{} ", row_num);
            for cell in row {
                let mark = match cell {
                    0 => "X",
                    1 => "O",
                    _ => " ",
                };
                print!("|  {}  ", mark)
            }
            println!("|");
            print!("  ├-----┼-----┼-----┤");
            row_num += 1;
        }
        print!("\r");
        println!("  └-----┴-----┴-----┘");
    }

    fn is_my_turn(&self, my_identity: SuiAddress) -> bool {
        let current_player = if self.cur_turn % 2 == 0 {
            self.x_address
        } else {
            self.o_address
        };
        current_player == my_identity
    }
}
