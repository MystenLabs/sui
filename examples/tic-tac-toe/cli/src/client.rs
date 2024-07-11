// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use clap::Parser;
use move_core_types::language_storage::StructTag;
use shared_crypto::intent::Intent;
use sui_keys::keystore::AccountKeystore;
use sui_sdk::{
    rpc_types::{
        DevInspectArgs, DevInspectResults, DryRunTransactionBlockResponse, ObjectChange, SuiData,
        SuiExecutionStatus, SuiObjectData, SuiObjectDataFilter, SuiObjectDataOptions,
        SuiObjectResponse, SuiObjectResponseQuery, SuiProtocolConfigValue,
        SuiTransactionBlockEffectsAPI, SuiTransactionBlockResponse,
    },
    wallet_context::WalletContext,
    SuiClient,
};
use sui_types::{
    base_types::{ObjectID, ObjectRef, SuiAddress},
    crypto::PublicKey,
    multisig::{MultiSig, MultiSigPublicKey},
    object::Owner,
    programmable_transaction_builder::ProgrammableTransactionBuilder,
    signature::GenericSignature,
    transaction::{
        InputObjectKind, ObjectArg, ProgrammableTransaction, Transaction, TransactionData,
        TransactionKind,
    },
    Identifier,
};

use crate::{
    crypto::combine_keys,
    game::{self, Game, GameKind, Winner},
    turn_cap::TurnCap,
};

#[derive(Parser, Debug)]
pub struct Connection {
    /// The Sui CLI config file, (default: ~/.sui/sui_config/client.yaml)
    #[clap(long)]
    config: Option<PathBuf>,

    /// Object ID of the game's package.
    #[clap(long, short, env = "PKG")]
    package_id: ObjectID,
}

pub(crate) struct Client {
    wallet: WalletContext,
    package: ObjectID,
}

impl Client {
    /// Create a new client that derives its active address and RPC from the CLI's config (found at
    /// path `config`), and that expects to interact with the tic-tac-toe package at address
    /// `package`.
    pub(crate) fn new(conn: Connection) -> Result<Self> {
        let Some(config) = conn.config.or_else(|| {
            let mut default = dirs::home_dir()?;
            default.extend([".sui", "sui_config", "client.yaml"]);
            Some(default)
        }) else {
            bail!(
                "Cannot find wallet config. No config was supplied, and the default path \
                 (~/.sui/sui_config/client.yaml) does not exist.",
            );
        };

        let wallet = WalletContext::new(&config, None, None)?;
        Ok(Self {
            wallet,
            package: conn.package_id,
        })
    }

    /// Fetch the details of a game object from on-chain (can be either shared or owned).
    pub(crate) async fn game(&self, id: ObjectID) -> Result<Game> {
        let client = self.client().await?;

        // (1) Read from RPC
        let response = client
            .read_api()
            .get_object_with_options(
                id,
                SuiObjectDataOptions {
                    show_owner: true,
                    show_bcs: true,
                    ..Default::default()
                },
            )
            .await
            .context("Error fetching game over RPC.")?;

        if let Some(err) = response.error {
            bail!(err);
        }

        // (2) Perform validation checks
        let Some(SuiObjectData {
            object_id,
            version,
            digest,
            bcs: Some(raw),
            owner: Some(owner),
            ..
        }) = response.data
        else {
            bail!("INTERNAL ERROR: No data for game.");
        };

        let Some(raw) = raw.try_as_move() else {
            bail!("It is a package, not an object.");
        };

        if raw.type_.name.as_str() != "Game" {
            bail!("It is not a Game object, it has type {}.", raw.type_);
        }

        let package = ObjectID::from(raw.type_.address);
        if package != self.package {
            bail!(
                "It is expected to be from package {} but is from package {}.",
                self.package,
                package,
            );
        }

        // (3) Deserialize contents
        let kind = match raw.type_.module.as_str() {
            "shared" => GameKind::Shared(
                bcs::from_bytes(&raw.bcs_bytes).context("Failed to deserialize contents.")?,
            ),

            "owned" => GameKind::Owned(
                bcs::from_bytes(&raw.bcs_bytes).context("Failed to deserialize contents.")?,
            ),

            kind => bail!("{id} has unrecognised Game kind: {kind}."),
        };

        // (4) Check whether the game has ended or not.
        let mut builder = ProgrammableTransactionBuilder::new();
        let g = if let Owner::Shared {
            initial_shared_version,
        } = owner
        {
            builder.obj(ObjectArg::SharedObject {
                id,
                initial_shared_version,
                mutable: false,
            })?
        } else {
            builder.obj(ObjectArg::ImmOrOwnedObject((object_id, version, digest)))?
        };

        builder.programmable_move_call(
            self.package,
            raw.type_.module.clone(),
            Identifier::new("ended").unwrap(),
            vec![],
            vec![g],
        );

        let results = client
            .read_api()
            .dev_inspect_transaction_block(
                SuiAddress::ZERO,
                TransactionKind::ProgrammableTransaction(builder.finish()),
                None,
                None,
                Some(DevInspectArgs {
                    skip_checks: Some(true),
                    ..Default::default()
                }),
            )
            .await
            .context("Error checking game winner.")?;

        fn extract_winner(results: &DevInspectResults) -> Option<Winner> {
            match *results
                .results
                .as_ref()?
                .first()?
                .return_values
                .first()?
                .0
                .first()?
            {
                0 => Some(Winner::None),
                1 => Some(Winner::Draw),
                2 => Some(Winner::Win),
                _ => None,
            }
        }

        let Some(winner) = extract_winner(&results) else {
            bail!("Error checking game winner.");
        };

        Ok(Game {
            kind,
            owner,
            version,
            digest,
            winner,
        })
    }

    /// Look for a `TurnCap` for the given `game` owned by the wallet's active address, and return
    /// its `ObjectRef`. Fails if no such `TurnCap` can be found.
    pub(crate) async fn turn_cap(&mut self, game: &Game) -> Result<ObjectRef> {
        let player = self.wallet.active_address()?;
        let client = self.client().await?;
        let (game_id, _, _) = game.object_ref();

        let turn_cap_type = StructTag {
            address: self.package.into(),
            module: Identifier::new("owned").unwrap(),
            name: Identifier::new("TurnCap").unwrap(),
            type_params: vec![],
        };

        let query = Some(SuiObjectResponseQuery::new(
            Some(SuiObjectDataFilter::StructType(turn_cap_type.clone())),
            Some(SuiObjectDataOptions::new().with_bcs()),
        ));

        let mut cursor = None;
        loop {
            let response = client
                .read_api()
                .get_owned_objects(player, query.clone(), cursor, None)
                .await
                .context("Error fetching TurnCaps from RPC.")?;

            for SuiObjectResponse { data, error } in response.data {
                if let Some(err) = error {
                    bail!(err);
                }

                let Some(SuiObjectData {
                    object_id,
                    version,
                    digest,
                    bcs: Some(raw),
                    ..
                }) = data
                else {
                    continue;
                };

                let Some(raw) = raw.try_as_move() else {
                    continue;
                };

                if raw.type_ != turn_cap_type {
                    continue;
                }

                let turn_cap: TurnCap = bcs::from_bytes(&raw.bcs_bytes)
                    .context("INTERNAL ERROR: Failed to deserialize TurnCap.")?;

                if turn_cap.game == game_id {
                    return Ok((object_id, version, digest));
                }
            }

            cursor = response.next_cursor;
            if !response.has_next_page {
                bail!("Could not find TurnCap. Is it your turn?");
            }
        }
    }

    /// Create a new shared game, between the wallet's active address and the given `opponent`.
    /// Returns the ID of the Game that was created on success.
    pub(crate) async fn new_shared_game(&mut self, opponent: SuiAddress) -> Result<ObjectID> {
        let player = self.wallet.active_address()?;

        let mut builder = ProgrammableTransactionBuilder::new();
        let x = builder.pure(player)?;
        let o = builder.pure(opponent)?;

        builder.programmable_move_call(
            self.package,
            Identifier::new("shared").unwrap(),
            Identifier::new("new").unwrap(),
            vec![],
            vec![x, o],
        );

        let tx = self.build_tx_data(player, builder.finish()).await?;
        self.execute_for_game(tx).await
    }

    /// Create a new owned game, between the wallet's active address and the given `opponent`. The
    /// game is transferred to a 1-of-2 multisig address -- the admin -- where the two partial
    /// signatures are the player's and the opponent's.
    ///
    /// Returns the ID for the Game that was created on success.
    pub async fn new_owned_game(&mut self, opponent_key: PublicKey) -> Result<ObjectID> {
        let player = self.wallet.active_address()?;
        let player_key = self.wallet.config.keystore.get_key(&player)?.public();

        // The opponent's address can be derived from their public key, but not vice versa.
        let opponent = SuiAddress::from(&opponent_key);

        // A 1-of-2 multisig acts as the admin of the game. The Game object will be transferred to
        // this address once it is created.
        let admin_key = combine_keys(vec![player_key, opponent_key])?;
        let admin = SuiAddress::from(&admin_key);
        let admin_bytes =
            bcs::to_bytes(&admin_key).context("INTERNAL ERROR: Failed to encode admin key.")?;

        let mut builder = ProgrammableTransactionBuilder::new();
        let x = builder.pure(player)?;
        let o = builder.pure(opponent)?;
        let a = builder.pure(admin_bytes)?;

        let game = builder.programmable_move_call(
            self.package,
            Identifier::new("owned").unwrap(),
            Identifier::new("new").unwrap(),
            vec![],
            vec![x, o, a],
        );

        builder.transfer_arg(admin, game);

        let tx = self.build_tx_data(player, builder.finish()).await?;
        self.execute_for_game(tx).await
    }

    /// Delete a shared game, given itself contents and its ownership information (which should be a
    /// `Owner::Shared`).
    pub async fn delete_shared_game(&mut self, game: &game::Shared, owner: Owner) -> Result<()> {
        let player = self.wallet.active_address()?;

        let Owner::Shared {
            initial_shared_version,
        } = owner
        else {
            bail!("Game is not shared");
        };

        let mut builder = ProgrammableTransactionBuilder::new();

        let g = builder.obj(ObjectArg::SharedObject {
            id: game.board.id,
            initial_shared_version,
            mutable: true,
        })?;

        builder.programmable_move_call(
            self.package,
            Identifier::new("shared").unwrap(),
            Identifier::new("burn").unwrap(),
            vec![],
            vec![g],
        );

        let data = self.build_tx_data(player, builder.finish()).await?;
        let tx = self.wallet.sign_transaction(&data);
        self.execute_transaction(tx).await?;
        Ok(())
    }

    /// Delete an owned (multi-sig) game. The transaction is signed by the player on behalf of the
    /// admin (multi-sig) address, and also directly by the player who is acting as the sponsor.
    pub async fn delete_owned_game(
        &mut self,
        game: &game::Owned,
        game_ref: ObjectRef,
    ) -> Result<()> {
        let player = self.wallet.active_address()?;

        let mut builder = ProgrammableTransactionBuilder::new();

        let g = builder.obj(ObjectArg::ImmOrOwnedObject(game_ref))?;

        builder.programmable_move_call(
            self.package,
            Identifier::new("owned").unwrap(),
            Identifier::new("burn").unwrap(),
            vec![],
            vec![g],
        );

        let admin_key: MultiSigPublicKey =
            bcs::from_bytes(&game.admin).context("Failed to deserialize admin's public key.")?;
        let admin = SuiAddress::from(&admin_key);

        let data = self
            .build_tx_data_with_sponsor(admin, Some(player), builder.finish())
            .await?;

        let tx = self
            .multi_sig_transaction(player, admin_key, data)
            .await
            .context("Failed multi-sign transaction.")?;

        self.execute_transaction(tx).await?;
        Ok(())
    }

    /// Make a move on a shared game as the wallet's active address. Fails if the active address is
    /// not meant to make the next move, or if the position is already occupied.
    pub async fn make_shared_move(
        &mut self,
        game: &game::Shared,
        owner: Owner,
        row: u8,
        col: u8,
    ) -> Result<()> {
        let player = self.wallet.active_address()?;

        let Owner::Shared {
            initial_shared_version,
        } = owner
        else {
            bail!("Game is not shared");
        };

        let mut builder = ProgrammableTransactionBuilder::new();

        let g = builder.obj(ObjectArg::SharedObject {
            id: game.board.id,
            initial_shared_version,
            mutable: true,
        })?;

        let r = builder.pure(row)?;
        let c = builder.pure(col)?;

        builder.programmable_move_call(
            self.package,
            Identifier::new("shared").unwrap(),
            Identifier::new("place_mark").unwrap(),
            vec![],
            vec![g, r, c],
        );

        let data = self.build_tx_data(player, builder.finish()).await?;
        let tx = self.wallet.sign_transaction(&data);
        self.execute_transaction(tx).await?;
        Ok(())
    }

    /// Make a move on an owned game as the wallet's active address. This involves sending two
    /// transactions: The first from the player to create a `Mark`, and a second from the admin to
    /// receive the mark and apply it.
    pub async fn make_owned_move(
        &mut self,
        game: &game::Owned,
        game_ref: ObjectRef,
        cap_ref: ObjectRef,
        row: u8,
        col: u8,
    ) -> Result<()> {
        let player = self.wallet.active_address()?;

        // First transaction sends the mark to the game.
        let mut builder = ProgrammableTransactionBuilder::new();

        let t = builder.obj(ObjectArg::ImmOrOwnedObject(cap_ref))?;
        let r = builder.pure(row)?;
        let c = builder.pure(col)?;

        builder.programmable_move_call(
            self.package,
            Identifier::new("owned").unwrap(),
            Identifier::new("send_mark").unwrap(),
            vec![],
            vec![t, r, c],
        );

        let data = self.build_tx_data(player, builder.finish()).await?;
        let tx = self.wallet.sign_transaction(&data);
        let SuiTransactionBlockResponse {
            object_changes: Some(object_changes),
            ..
        } = self
            .execute_transaction(tx)
            .await
            .context("Failed to send mark.")?
        else {
            bail!("Can't find Mark.");
        };

        let Some(mark) = object_changes.into_iter().find_map(|change| {
            let ObjectChange::Created {
                object_type,
                object_id,
                version,
                digest,
                ..
            } = change
            else {
                return None;
            };

            if ObjectID::from(object_type.address) != self.package {
                return None;
            }

            if object_type.name.as_str() != "Mark" {
                return None;
            }

            Some((object_id, version, digest))
        }) else {
            bail!("Can't find Mark");
        };

        // Second transaction applies the mark to the game, and needs to be run as the admin.
        let mut builder = ProgrammableTransactionBuilder::new();

        let g = builder.obj(ObjectArg::ImmOrOwnedObject(game_ref))?;
        let m = builder.obj(ObjectArg::Receiving(mark))?;

        builder.programmable_move_call(
            self.package,
            Identifier::new("owned").unwrap(),
            Identifier::new("place_mark").unwrap(),
            vec![],
            vec![g, m],
        );

        let admin_key: MultiSigPublicKey =
            bcs::from_bytes(&game.admin).context("Failed to deserialize admin's public key.")?;
        let admin = SuiAddress::from(&admin_key);

        let data = self
            .build_tx_data_with_sponsor(admin, Some(player), builder.finish())
            .await?;

        let tx = self
            .multi_sig_transaction(player, admin_key, data)
            .await
            .context("Failed multi-sign transaction.")?;

        self.execute_transaction(tx)
            .await
            .context("Failed to place mark.")?;

        Ok(())
    }

    /// Execute a PTB, expecting it to create a shared or owned Game, and return its ObjectID.
    async fn execute_for_game(&self, data: TransactionData) -> Result<ObjectID> {
        let tx = self.wallet.sign_transaction(&data);
        let SuiTransactionBlockResponse {
            object_changes: Some(object_changes),
            ..
        } = self.execute_transaction(tx).await?
        else {
            bail!("Can't find Game ID");
        };

        let Some(game_id) = object_changes.into_iter().find_map(|change| {
            let ObjectChange::Created {
                object_type,
                object_id,
                ..
            } = change
            else {
                return None;
            };

            if ObjectID::from(object_type.address) != self.package {
                return None;
            }

            if object_type.name.as_str() != "Game" {
                return None;
            }

            Some(object_id)
        }) else {
            bail!("Can't find Game ID");
        };

        Ok(game_id)
    }

    /// Like `build_tx_data_with_sponsor`, but without a sponsor.
    async fn build_tx_data(
        &self,
        sender: SuiAddress,
        tx: ProgrammableTransaction,
    ) -> Result<TransactionData> {
        self.build_tx_data_with_sponsor(sender, None, tx).await
    }

    /// Do gas estimation and coin selection to create a `TransactionData` from a
    /// `ProgrammableTransaction`. If `sponsor` is provided, it will be used as the gas sponsor, and
    /// coin selection will fetch coins owned by this address, otherwise coins will be selected from
    /// the `sender`'s owned objects.
    async fn build_tx_data_with_sponsor(
        &self,
        sender: SuiAddress,
        sponsor: Option<SuiAddress>,
        tx: ProgrammableTransaction,
    ) -> Result<TransactionData> {
        let client = self.client().await?;

        let max_budget = self.max_gas_budget().await?;

        let gas_price = self
            .wallet
            .get_reference_gas_price()
            .await
            .context("Error fetching reference gas price")?;

        let tx_kind = TransactionKind::ProgrammableTransaction(tx);

        // Gas Estimation
        let tx_data = client
            .transaction_builder()
            .tx_data_for_dry_run(
                sender,
                tx_kind.clone(),
                max_budget,
                gas_price,
                /* gas_payment */ None,
                /* gas_sponsor */ None,
            )
            .await;

        let DryRunTransactionBlockResponse { effects, .. } = client
            .read_api()
            .dry_run_transaction_block(tx_data.clone())
            .await
            .context("Error estimating gas budget")?;

        let gas_used = effects.gas_cost_summary();
        let overhead = 1000 * gas_price;
        let net_used = gas_used.net_gas_usage();
        let computation = gas_used.computation_cost;

        let budget = overhead + (net_used.max(0) as u64).max(computation);

        let gas_coin = self
            .select_coins(sponsor.unwrap_or(sender), budget, &tx_kind)
            .await?;

        let payment = vec![gas_coin];
        Ok(if let Some(sponsor) = sponsor {
            TransactionData::new_with_gas_coins_allow_sponsor(
                tx_kind, sender, payment, budget, gas_price, sponsor,
            )
        } else {
            TransactionData::new_with_gas_coins(tx_kind, sender, payment, budget, gas_price)
        })
    }

    /// Find the max budget allowed for a transaction according to the current protocol config.
    async fn max_gas_budget(&self) -> Result<u64> {
        let client = self.client().await?;

        let cfg = client.read_api().get_protocol_config(None).await?;
        let Some(Some(SuiProtocolConfigValue::U64(max))) = cfg.attributes.get("max_tx_gas") else {
            bail!("Couldn't find max gas budget");
        };

        Ok(*max)
    }

    /// Select Gas coins owned by `owner` to meet `balance`, avoiding input objects to the
    /// transaction, `tx`.
    async fn select_coins(
        &self,
        owner: SuiAddress,
        balance: u64,
        tx: &TransactionKind,
    ) -> Result<ObjectRef> {
        let exclude = tx
            .input_objects()?
            .into_iter()
            .filter_map(|input| match input {
                InputObjectKind::ImmOrOwnedMoveObject((id, _, _)) => Some(id),
                InputObjectKind::MovePackage(_) => None,
                InputObjectKind::SharedMoveObject { .. } => None,
            })
            .collect();

        Ok(self
            .wallet
            .gas_for_owner_budget(owner, balance, exclude)
            .await?
            .1
            .object_ref())
    }

    /// Sign the transaction as `sender` by itself (as the sponsor) and as part of the multi-sig,
    /// `admin_key` (the transaction sender), and execute it.
    async fn multi_sig_transaction(
        &self,
        sender: SuiAddress,
        admin_key: MultiSigPublicKey,
        data: TransactionData,
    ) -> Result<Transaction> {
        let sponsor_sig: GenericSignature = self
            .wallet
            .config
            .keystore
            .sign_secure(&sender, &data, Intent::sui_transaction())
            .context("Signing transaction")?
            .into();

        let multi_sig: GenericSignature = MultiSig::combine(vec![sponsor_sig.clone()], admin_key)
            .context("Signing as admin")?
            .into();

        Ok(Transaction::from_generic_sig_data(
            data,
            vec![multi_sig, sponsor_sig],
        ))
    }

    /// Execute the transaction, and check whether it succeeded or failed. Transaction execution
    /// failure is treated as an error.
    async fn execute_transaction(&self, tx: Transaction) -> Result<SuiTransactionBlockResponse> {
        let response = self
            .wallet
            .execute_transaction_may_fail(tx)
            .await
            .context("Error executing transaction")?;

        let Some(effects) = &response.effects else {
            bail!("Failed to find effects for transaction");
        };

        if let SuiExecutionStatus::Failure { error } = effects.status() {
            bail!(error.to_owned());
        }

        Ok(response)
    }

    async fn client(&self) -> Result<SuiClient> {
        self.wallet
            .get_client()
            .await
            .context("Error fetching client")
    }
}
