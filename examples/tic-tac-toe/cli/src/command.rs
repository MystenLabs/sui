// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::str::FromStr;

use anyhow::{ensure, Context, Result};
use clap::Parser;
use sui_types::base_types::{ObjectID, SuiAddress};

use crate::{
    client::{Client, Connection},
    crypto::public_key_from_base64,
    game::GameKind,
};

#[derive(Parser, Debug)]
pub enum Command {
    /// Start a new game of tic-tac-toe.
    New {
        /// Use the shared version of the game (default).
        #[clap(long, short)]
        shared: bool,

        /// Use the multi-sig version of the game.
        #[clap(long, short)]
        multi_sig: bool,

        /// For a shared game, this is the opponent's address. For a multi-sig game, it is their
        /// public key.
        opponent: String,

        #[clap(flatten)]
        conn: Connection,
    },

    /// Make a move in an existing game.
    Move {
        /// ID of the game to make a move on.
        game: ObjectID,

        /// The row to place the move in.
        #[clap(long, short)]
        row: u8,

        /// The column to place the move in.
        #[clap(long, short)]
        col: u8,

        #[clap(flatten)]
        conn: Connection,
    },

    /// Print the state of an existing game.
    View {
        /// ID of the game to view.
        game: ObjectID,

        #[clap(flatten)]
        conn: Connection,
    },

    /// Delete a finished game.
    Delete {
        /// ID of the game to view.
        game: ObjectID,

        #[clap(flatten)]
        conn: Connection,
    },
}

impl Command {
    /// Ensure the parameters for the command are valid.
    fn validate(&mut self) -> Result<()> {
        match self {
            Command::New {
                shared, multi_sig, ..
            } => {
                ensure!(
                    !*shared || !*multi_sig,
                    "Cannot specify both shared and multi-sig"
                );
                if !*shared && !*multi_sig {
                    *shared = true;
                }
            }

            Command::Move { row, col, .. } => {
                ensure!(*row < 3, "Row must be between 0 and 2");
                ensure!(*col < 3, "Column must be between 0 and 2");
            }

            Command::View { .. } => {}
            Command::Delete { .. } => {}
        }

        Ok(())
    }

    pub async fn execute(mut self) -> Result<()> {
        self.validate()?;
        match self {
            Command::New {
                shared,
                multi_sig,
                opponent,
                conn,
            } => {
                let mut client = Client::new(conn)?;

                let game = if shared {
                    assert!(!multi_sig);
                    let opponent = SuiAddress::from_str(&opponent)
                        .with_context(|| format!("Invalid opponent address {opponent}"))?;

                    client.new_shared_game(opponent).await.with_context(|| {
                        format!("Error starting new shared game against {opponent}")
                    })?
                } else {
                    assert!(multi_sig);
                    let opponent_key = public_key_from_base64(&opponent).with_context(|| {
                        format!("Failed to decode opponent public key: {opponent}")
                    })?;

                    client.new_owned_game(opponent_key).await.with_context(|| {
                        format!("Error satarting new multi-sig game against {opponent}")
                    })?
                };

                let game = client
                    .game(game)
                    .await
                    .with_context(|| format!("Error fetching game {game}"))?;

                println!("{game}");
            }

            Command::Move {
                game,
                row,
                col,
                conn,
            } => {
                let mut client = Client::new(conn)?;

                let before = client
                    .game(game)
                    .await
                    .with_context(|| format!("Error fetching game {game}"))?;

                match &before.kind {
                    GameKind::Shared(game) => {
                        client
                            .make_shared_move(game, before.owner, row, col)
                            .await?;
                    }

                    GameKind::Owned(game) => {
                        let cap_ref = client
                            .turn_cap(&before)
                            .await
                            .context("Failed to find a TurnCap, is it your turn?")?;

                        client
                            .make_owned_move(game, before.object_ref(), cap_ref, row, col)
                            .await?;
                    }
                }

                let after = client
                    .game(game)
                    .await
                    .with_context(|| format!("Error fetching game {game}"))?;

                println!("{after}");
            }

            Command::View { game, conn } => {
                let client = Client::new(conn)?;
                let game = client
                    .game(game)
                    .await
                    .with_context(|| format!("Error fetching game {game}"))?;
                println!("{game}");
            }

            Command::Delete { game, conn } => {
                let mut client = Client::new(conn)?;

                let before = client
                    .game(game)
                    .await
                    .with_context(|| format!("Error fetching game {game}"))?;

                match &before.kind {
                    GameKind::Shared(game) => {
                        client.delete_shared_game(game, before.owner).await?;
                    }

                    GameKind::Owned(game) => {
                        client.delete_owned_game(game, before.object_ref()).await?;
                    }
                }

                println!("Deleted!");
            }
        }

        Ok(())
    }
}
