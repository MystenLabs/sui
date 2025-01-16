// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::{anyhow, Error};
use async_trait::async_trait;
use diesel::dsl::now;
use diesel::{ExpressionMethods, TextExpressionMethods};
use diesel::{OptionalExtension, QueryDsl, SelectableHelper};
use diesel_async::scoped_futures::ScopedFutureExt;
use diesel_async::AsyncConnection;
use diesel_async::RunQueryDsl;
use sui_indexer_builder::progress::ProgressSavingPolicy;
use sui_types::base_types::ObjectID;
use sui_types::transaction::{Command, TransactionDataAPI};
use tracing::info;

use sui_indexer_builder::indexer_builder::{DataMapper, IndexerProgressStore, Persistent};
use sui_indexer_builder::sui_datasource::CheckpointTxnData;
use sui_indexer_builder::{Task, Tasks, LIVE_TASK_TARGET_CHECKPOINT};
use sui_types::effects::TransactionEffectsAPI;
use sui_types::event::Event;
use sui_types::execution_status::ExecutionStatus;
use sui_types::full_checkpoint_content::CheckpointTransaction;

use crate::events::{
    MoveBalanceEvent, MoveFlashLoanBorrowedEvent, MoveOrderCanceledEvent, MoveOrderExpiredEvent,
    MoveOrderFilledEvent, MoveOrderModifiedEvent, MoveOrderPlacedEvent, MovePriceAddedEvent,
    MoveProposalEvent, MoveRebateEvent, MoveStakeEvent, MoveTradeParamsUpdateEvent, MoveVoteEvent,
};
use crate::metrics::DeepBookIndexerMetrics;
use crate::postgres_manager::PgPool;
use crate::schema::progress_store::{columns, dsl};
use crate::schema::{
    balances, flashloans, order_fills, order_updates, pool_prices, proposals, rebates, stakes,
    sui_error_transactions, trade_params_update, votes,
};
use crate::types::{
    Balances, Flashloan, OrderFill, OrderUpdate, OrderUpdateStatus, PoolPrice, ProcessedTxnData,
    Proposals, Rebates, Stakes, SuiTxnError, TradeParamsUpdate, Votes,
};
use crate::{models, schema};

/// Persistent layer impl
#[derive(Clone)]
pub struct PgDeepbookPersistent {
    pub pool: PgPool,
    save_progress_policy: ProgressSavingPolicy,
}

impl PgDeepbookPersistent {
    pub fn new(pool: PgPool, save_progress_policy: ProgressSavingPolicy) -> Self {
        Self {
            pool,
            save_progress_policy,
        }
    }

    async fn get_largest_backfill_task_target_checkpoint(
        &self,
        prefix: &str,
    ) -> Result<Option<u64>, Error> {
        let mut conn = self.pool.get().await?;
        let cp = dsl::progress_store
            .select(columns::target_checkpoint)
            // TODO: using like could be error prone, change the progress store schema to stare the task name properly.
            .filter(columns::task_name.like(format!("{prefix} - %")))
            .filter(columns::target_checkpoint.ne(i64::MAX))
            .order_by(columns::target_checkpoint.desc())
            .first::<i64>(&mut conn)
            .await
            .optional()?;
        Ok(cp.map(|c| c as u64))
    }
}

#[async_trait]
impl Persistent<ProcessedTxnData> for PgDeepbookPersistent {
    async fn write(&self, data: Vec<ProcessedTxnData>) -> Result<(), Error> {
        if data.is_empty() {
            return Ok(());
        }
        use futures::future;

        // Group data by type
        let mut order_updates_batch = vec![];
        let mut order_fills_batch = vec![];
        let mut flashloans_batch = vec![];
        let mut pool_prices_batch = vec![];
        let mut balances_batch = vec![];
        let mut proposals_batch = vec![];
        let mut rebates_batch = vec![];
        let mut stakes_batch = vec![];
        let mut trade_params_update_batch = vec![];
        let mut votes_batch = vec![];
        let mut error_transactions_batch = vec![];

        // Collect the data into batches
        for d in data {
            match d {
                ProcessedTxnData::OrderUpdate(t) => order_updates_batch.push(t.to_db()),
                ProcessedTxnData::OrderFill(t) => order_fills_batch.push(t.to_db()),
                ProcessedTxnData::Flashloan(t) => flashloans_batch.push(t.to_db()),
                ProcessedTxnData::PoolPrice(t) => pool_prices_batch.push(t.to_db()),
                ProcessedTxnData::Balances(t) => balances_batch.push(t.to_db()),
                ProcessedTxnData::Proposals(t) => proposals_batch.push(t.to_db()),
                ProcessedTxnData::Rebates(t) => rebates_batch.push(t.to_db()),
                ProcessedTxnData::Stakes(t) => stakes_batch.push(t.to_db()),
                ProcessedTxnData::TradeParamsUpdate(t) => trade_params_update_batch.push(t.to_db()),
                ProcessedTxnData::Votes(t) => votes_batch.push(t.to_db()),
                ProcessedTxnData::Error(e) => error_transactions_batch.push(e.to_db()),
            }
        }

        let connection = &mut self.pool.get().await?;
        connection
            .transaction(|conn| {
                async move {
                    // Create async tasks for each batch insert
                    let mut tasks = Vec::new();

                    if !order_updates_batch.is_empty() {
                        tasks.push(
                            diesel::insert_into(order_updates::table)
                                .values(&order_updates_batch)
                                .on_conflict_do_nothing()
                                .execute(conn),
                        );
                    }
                    if !order_fills_batch.is_empty() {
                        tasks.push(
                            diesel::insert_into(order_fills::table)
                                .values(&order_fills_batch)
                                .on_conflict_do_nothing()
                                .execute(conn),
                        );
                    }
                    if !flashloans_batch.is_empty() {
                        tasks.push(
                            diesel::insert_into(flashloans::table)
                                .values(&flashloans_batch)
                                .on_conflict_do_nothing()
                                .execute(conn),
                        );
                    }
                    if !pool_prices_batch.is_empty() {
                        tasks.push(
                            diesel::insert_into(pool_prices::table)
                                .values(&pool_prices_batch)
                                .on_conflict_do_nothing()
                                .execute(conn),
                        );
                    }
                    if !balances_batch.is_empty() {
                        tasks.push(
                            diesel::insert_into(balances::table)
                                .values(&balances_batch)
                                .on_conflict_do_nothing()
                                .execute(conn),
                        );
                    }
                    if !proposals_batch.is_empty() {
                        tasks.push(
                            diesel::insert_into(proposals::table)
                                .values(&proposals_batch)
                                .on_conflict_do_nothing()
                                .execute(conn),
                        );
                    }
                    if !rebates_batch.is_empty() {
                        tasks.push(
                            diesel::insert_into(rebates::table)
                                .values(&rebates_batch)
                                .on_conflict_do_nothing()
                                .execute(conn),
                        );
                    }
                    if !stakes_batch.is_empty() {
                        tasks.push(
                            diesel::insert_into(stakes::table)
                                .values(&stakes_batch)
                                .on_conflict_do_nothing()
                                .execute(conn),
                        );
                    }
                    if !trade_params_update_batch.is_empty() {
                        tasks.push(
                            diesel::insert_into(trade_params_update::table)
                                .values(&trade_params_update_batch)
                                .on_conflict_do_nothing()
                                .execute(conn),
                        );
                    }
                    if !votes_batch.is_empty() {
                        tasks.push(
                            diesel::insert_into(votes::table)
                                .values(&votes_batch)
                                .on_conflict_do_nothing()
                                .execute(conn),
                        );
                    }
                    if !error_transactions_batch.is_empty() {
                        tasks.push(
                            diesel::insert_into(sui_error_transactions::table)
                                .values(&error_transactions_batch)
                                .on_conflict_do_nothing()
                                .execute(conn),
                        );
                    }

                    // Execute all tasks concurrently
                    let _: Vec<_> = future::try_join_all(tasks).await?;

                    Ok(())
                }
                .scope_boxed()
            })
            .await
    }
}

#[async_trait]
impl IndexerProgressStore for PgDeepbookPersistent {
    async fn load_progress(&self, task_name: String) -> anyhow::Result<u64> {
        let mut conn = self.pool.get().await?;
        let cp: Option<models::ProgressStore> = dsl::progress_store
            .find(&task_name)
            .select(models::ProgressStore::as_select())
            .first(&mut conn)
            .await
            .optional()?;
        Ok(cp
            .ok_or(anyhow!("Cannot found progress for task {task_name}"))?
            .checkpoint as u64)
    }

    async fn save_progress(
        &mut self,
        task: &Task,
        checkpoint_numbers: &[u64],
    ) -> anyhow::Result<Option<u64>> {
        if checkpoint_numbers.is_empty() {
            return Ok(None);
        }
        let task_name = task.task_name.clone();
        if let Some(checkpoint_to_save) = self
            .save_progress_policy
            .cache_progress(task, checkpoint_numbers)
        {
            let mut conn = self.pool.get().await?;
            diesel::insert_into(schema::progress_store::table)
                .values(&models::ProgressStore {
                    task_name,
                    checkpoint: checkpoint_to_save as i64,
                    // Target checkpoint and timestamp will only be written for new entries
                    target_checkpoint: i64::MAX,
                    // Timestamp is defaulted to current time in DB if None
                    timestamp: None,
                })
                .on_conflict(dsl::task_name)
                .do_update()
                .set((
                    columns::checkpoint.eq(checkpoint_to_save as i64),
                    columns::timestamp.eq(now),
                ))
                .execute(&mut conn)
                .await?;
            // TODO: add metrics here
            return Ok(Some(checkpoint_to_save));
        }
        Ok(None)
    }

    async fn get_ongoing_tasks(&self, prefix: &str) -> Result<Tasks, anyhow::Error> {
        let mut conn = self.pool.get().await?;
        // get all unfinished tasks
        let cp: Vec<models::ProgressStore> = dsl::progress_store
            // TODO: using like could be error prone, change the progress store schema to stare the task name properly.
            .filter(columns::task_name.like(format!("{prefix} - %")))
            .filter(columns::checkpoint.lt(columns::target_checkpoint))
            .order_by(columns::target_checkpoint.desc())
            .load(&mut conn)
            .await?;
        let tasks = cp.into_iter().map(|d| d.into()).collect();
        Ok(Tasks::new(tasks)?)
    }

    async fn get_largest_indexed_checkpoint(&self, prefix: &str) -> Result<Option<u64>, Error> {
        let mut conn = self.pool.get().await?;
        let cp = dsl::progress_store
            .select(columns::checkpoint)
            // TODO: using like could be error prone, change the progress store schema to stare the task name properly.
            .filter(columns::task_name.like(format!("{prefix} - %")))
            .filter(columns::target_checkpoint.eq(i64::MAX))
            .first::<i64>(&mut conn)
            .await
            .optional()?;

        if let Some(cp) = cp {
            Ok(Some(cp as u64))
        } else {
            // Use the largest backfill target checkpoint as a fallback
            self.get_largest_backfill_task_target_checkpoint(prefix)
                .await
        }
    }

    async fn register_task(
        &mut self,
        task_name: String,
        checkpoint: u64,
        target_checkpoint: u64,
    ) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get().await?;
        diesel::insert_into(schema::progress_store::table)
            .values(models::ProgressStore {
                task_name,
                checkpoint: checkpoint as i64,
                target_checkpoint: target_checkpoint as i64,
                // Timestamp is defaulted to current time in DB if None
                timestamp: None,
            })
            .execute(&mut conn)
            .await?;
        Ok(())
    }

    /// Register a live task to progress store with a start checkpoint.
    async fn register_live_task(
        &mut self,
        task_name: String,
        start_checkpoint: u64,
    ) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get().await?;
        diesel::insert_into(schema::progress_store::table)
            .values(models::ProgressStore {
                task_name,
                checkpoint: start_checkpoint as i64,
                target_checkpoint: LIVE_TASK_TARGET_CHECKPOINT,
                // Timestamp is defaulted to current time in DB if None
                timestamp: None,
            })
            .execute(&mut conn)
            .await?;
        Ok(())
    }

    async fn update_task(&mut self, task: Task) -> Result<(), anyhow::Error> {
        let mut conn = self.pool.get().await?;
        diesel::update(dsl::progress_store.filter(columns::task_name.eq(task.task_name)))
            .set((
                columns::checkpoint.eq(task.start_checkpoint as i64),
                columns::target_checkpoint.eq(task.target_checkpoint as i64),
                columns::timestamp.eq(now),
            ))
            .execute(&mut conn)
            .await?;
        Ok(())
    }
}

/// Data mapper impl
#[derive(Clone)]
pub struct SuiDeepBookDataMapper {
    pub metrics: DeepBookIndexerMetrics,
    pub package_id: ObjectID,
}

impl DataMapper<CheckpointTxnData, ProcessedTxnData> for SuiDeepBookDataMapper {
    fn map(
        &self,
        (data, checkpoint_num, timestamp_ms): CheckpointTxnData,
    ) -> Result<Vec<ProcessedTxnData>, Error> {
        if !data.input_objects.iter().any(|obj| {
            obj.data
                .type_()
                .map(|t| t.address() == self.package_id.into())
                .unwrap_or_default()
        }) {
            return Ok(vec![]);
        }

        self.metrics.total_deepbook_transactions.inc();

        match &data.events {
            Some(events) => {
                let processed_sui_events =
                    events
                        .data
                        .iter()
                        .enumerate()
                        .try_fold(vec![], |mut result, (i, ev)| {
                            if let Some(data) = process_sui_event(
                                ev,
                                i,
                                &data,
                                checkpoint_num,
                                timestamp_ms,
                                self.package_id,
                            )? {
                                result.push(data);
                            }
                            Ok::<_, anyhow::Error>(result)
                        })?;
                if !processed_sui_events.is_empty() {
                    info!(
                        "SUI: Extracted {} deepbook data entries for tx {}.",
                        processed_sui_events.len(),
                        data.transaction.digest()
                    );
                }
                Ok(processed_sui_events)
            }
            None => {
                if let ExecutionStatus::Failure { error, command } = data.effects.status() {
                    let txn_kind = data.transaction.transaction_data().clone().into_kind();
                    let first_command = txn_kind.iter_commands().next();
                    let package = if let Some(Command::MoveCall(move_call)) = first_command {
                        move_call.package.to_string()
                    } else {
                        "".to_string()
                    };
                    Ok(vec![ProcessedTxnData::Error(SuiTxnError {
                        tx_digest: *data.transaction.digest(),
                        sender: data.transaction.sender_address(),
                        timestamp_ms,
                        failure_status: error.to_string(),
                        package,
                        cmd_idx: command.map(|idx| idx as u64),
                    })])
                } else {
                    Ok(vec![])
                }
            }
        }
    }
}

fn process_sui_event(
    ev: &Event,
    event_index: usize,
    tx: &CheckpointTransaction,
    checkpoint: u64,
    checkpoint_timestamp_ms: u64,
    package_id: ObjectID,
) -> Result<Option<ProcessedTxnData>, anyhow::Error> {
    Ok(if ev.type_.address == *package_id {
        match ev.type_.name.as_str() {
            "OrderPlaced" => {
                let move_event: MoveOrderPlacedEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let txn_data = Some(ProcessedTxnData::OrderUpdate(OrderUpdate {
                    digest: tx.transaction.digest().to_string(),
                    sender: tx.transaction.sender_address().to_string(),
                    event_digest,
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    status: OrderUpdateStatus::Placed,
                    pool_id: move_event.pool_id.to_string(),
                    order_id: move_event.order_id,
                    client_order_id: move_event.client_order_id,
                    price: move_event.price,
                    is_bid: move_event.is_bid,
                    onchain_timestamp: move_event.timestamp,
                    original_quantity: move_event.placed_quantity,
                    quantity: move_event.placed_quantity,
                    filled_quantity: 0,
                    trader: move_event.trader.to_string(),
                    balance_manager_id: move_event.balance_manager_id.to_string(),
                }));
                info!("Observed Deepbook Order Placed {:?}", txn_data);

                txn_data
            }
            "OrderModified" => {
                let move_event: MoveOrderModifiedEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let txn_data = Some(ProcessedTxnData::OrderUpdate(OrderUpdate {
                    digest: tx.transaction.digest().to_string(),
                    event_digest,
                    sender: tx.transaction.sender_address().to_string(),
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    status: OrderUpdateStatus::Modified,
                    pool_id: move_event.pool_id.to_string(),
                    order_id: move_event.order_id,
                    client_order_id: move_event.client_order_id,
                    price: move_event.price,
                    is_bid: move_event.is_bid,
                    onchain_timestamp: move_event.timestamp,
                    original_quantity: move_event.previous_quantity,
                    quantity: move_event.new_quantity,
                    filled_quantity: move_event.filled_quantity,
                    trader: move_event.trader.to_string(),
                    balance_manager_id: move_event.balance_manager_id.to_string(),
                }));
                info!("Observed Deepbook Order Modified {:?}", txn_data);

                txn_data
            }
            "OrderCanceled" => {
                let move_event: MoveOrderCanceledEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let txn_data = Some(ProcessedTxnData::OrderUpdate(OrderUpdate {
                    digest: tx.transaction.digest().to_string(),
                    event_digest,
                    sender: tx.transaction.sender_address().to_string(),
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    status: OrderUpdateStatus::Canceled,
                    pool_id: move_event.pool_id.to_string(),
                    order_id: move_event.order_id,
                    client_order_id: move_event.client_order_id,
                    price: move_event.price,
                    is_bid: move_event.is_bid,
                    onchain_timestamp: move_event.timestamp,
                    original_quantity: move_event.original_quantity,
                    quantity: move_event.base_asset_quantity_canceled,
                    filled_quantity: move_event.original_quantity
                        - move_event.base_asset_quantity_canceled,
                    trader: move_event.trader.to_string(),
                    balance_manager_id: move_event.balance_manager_id.to_string(),
                }));
                info!("Observed Deepbook Order Canceled {:?}", txn_data);

                txn_data
            }
            "OrderExpired" => {
                let move_event: MoveOrderExpiredEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let txn_data = Some(ProcessedTxnData::OrderUpdate(OrderUpdate {
                    digest: tx.transaction.digest().to_string(),
                    event_digest,
                    sender: tx.transaction.sender_address().to_string(),
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    status: OrderUpdateStatus::Expired,
                    pool_id: move_event.pool_id.to_string(),
                    order_id: move_event.order_id,
                    client_order_id: move_event.client_order_id,
                    price: move_event.price,
                    is_bid: move_event.is_bid,
                    onchain_timestamp: move_event.timestamp,
                    original_quantity: move_event.original_quantity,
                    quantity: move_event.base_asset_quantity_canceled,
                    filled_quantity: move_event.original_quantity
                        - move_event.base_asset_quantity_canceled,
                    trader: move_event.trader.to_string(),
                    balance_manager_id: move_event.balance_manager_id.to_string(),
                }));
                info!("Observed Deepbook Order Expired {:?}", txn_data);

                txn_data
            }
            "OrderFilled" => {
                let move_event: MoveOrderFilledEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let txn_data = Some(ProcessedTxnData::OrderFill(OrderFill {
                    digest: tx.transaction.digest().to_string(),
                    event_digest,
                    sender: tx.transaction.sender_address().to_string(),
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    pool_id: move_event.pool_id.to_string(),
                    maker_order_id: move_event.maker_order_id,
                    taker_order_id: move_event.taker_order_id,
                    maker_client_order_id: move_event.maker_client_order_id,
                    taker_client_order_id: move_event.taker_client_order_id,
                    price: move_event.price,
                    taker_is_bid: move_event.taker_is_bid,
                    taker_fee: move_event.taker_fee,
                    taker_fee_is_deep: move_event.taker_fee_is_deep,
                    maker_fee: move_event.maker_fee,
                    maker_fee_is_deep: move_event.maker_fee_is_deep,
                    base_quantity: move_event.base_quantity,
                    quote_quantity: move_event.quote_quantity,
                    maker_balance_manager_id: move_event.maker_balance_manager_id.to_string(),
                    taker_balance_manager_id: move_event.taker_balance_manager_id.to_string(),
                    onchain_timestamp: move_event.timestamp,
                }));
                info!("Observed Deepbook Order Filled {:?}", txn_data);

                txn_data
            }
            "FlashLoanBorrowed" => {
                let move_event: MoveFlashLoanBorrowedEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let txn_data = Some(ProcessedTxnData::Flashloan(Flashloan {
                    digest: tx.transaction.digest().to_string(),
                    event_digest,
                    sender: tx.transaction.sender_address().to_string(),
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    pool_id: move_event.pool_id.to_string(),
                    borrow_quantity: move_event.borrow_quantity,
                    borrow: true,
                    type_name: move_event.type_name.to_string(),
                }));
                info!("Observed Deepbook Flash Loan Borrowed {:?}", txn_data);

                txn_data
            }
            "PriceAdded" => {
                let move_event: MovePriceAddedEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let txn_data = Some(ProcessedTxnData::PoolPrice(PoolPrice {
                    digest: tx.transaction.digest().to_string(),
                    event_digest,
                    sender: tx.transaction.sender_address().to_string(),
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    target_pool: move_event.target_pool.to_string(),
                    conversion_rate: move_event.conversion_rate,
                    reference_pool: move_event.reference_pool.to_string(),
                }));
                info!("Observed Deepbook Price Addition {:?}", txn_data);

                txn_data
            }
            "BalanceEvent" => {
                let move_event: MoveBalanceEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let txn_data = Some(ProcessedTxnData::Balances(Balances {
                    digest: tx.transaction.digest().to_string(),
                    event_digest,
                    sender: tx.transaction.sender_address().to_string(),
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    balance_manager_id: move_event.balance_manager_id.to_string(),
                    asset: move_event.asset.to_string(),
                    amount: move_event.amount,
                    deposit: move_event.deposit,
                }));
                info!("Observed Deepbook Balance Event {:?}", txn_data);

                txn_data
            }
            "ProposalEvent" => {
                let move_event: MoveProposalEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let txn_data = Some(ProcessedTxnData::Proposals(Proposals {
                    digest: tx.transaction.digest().to_string(),
                    event_digest,
                    sender: tx.transaction.sender_address().to_string(),
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    pool_id: move_event.pool_id.to_string(),
                    balance_manager_id: move_event.balance_manager_id.to_string(),
                    epoch: move_event.epoch,
                    taker_fee: move_event.taker_fee,
                    maker_fee: move_event.maker_fee,
                    stake_required: move_event.stake_required,
                }));
                info!("Observed Deepbook Proposal Event {:?}", txn_data);

                txn_data
            }
            "RebateEvent" => {
                let move_event: MoveRebateEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let txn_data = Some(ProcessedTxnData::Rebates(Rebates {
                    digest: tx.transaction.digest().to_string(),
                    event_digest,
                    sender: tx.transaction.sender_address().to_string(),
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    pool_id: move_event.pool_id.to_string(),
                    balance_manager_id: move_event.balance_manager_id.to_string(),
                    epoch: move_event.epoch,
                    claim_amount: move_event.claim_amount,
                }));
                info!("Observed Deepbook Rebate Event {:?}", txn_data);

                txn_data
            }
            "StakeEvent" => {
                let move_event: MoveStakeEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let txn_data = Some(ProcessedTxnData::Stakes(Stakes {
                    digest: tx.transaction.digest().to_string(),
                    event_digest,
                    sender: tx.transaction.sender_address().to_string(),
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    pool_id: move_event.pool_id.to_string(),
                    balance_manager_id: move_event.balance_manager_id.to_string(),
                    epoch: move_event.epoch,
                    amount: move_event.amount,
                    stake: move_event.stake,
                }));
                info!("Observed Deepbook Stake Event {:?}", txn_data);

                txn_data
            }
            "TradeParamsUpdateEvent" => {
                let move_event: MoveTradeParamsUpdateEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let shared_objects = &tx.input_objects;
                let mut pool_id = "0x0".to_string();
                for obj in shared_objects.iter() {
                    if let Some(obj_type) = obj.data.type_() {
                        if obj_type.module().to_string().eq("pool")
                            && obj_type.address() == *package_id
                        {
                            pool_id = obj_type.address().to_string();
                            break;
                        }
                    }
                }
                let txn_data = Some(ProcessedTxnData::TradeParamsUpdate(TradeParamsUpdate {
                    digest: tx.transaction.digest().to_string(),
                    event_digest,
                    sender: tx.transaction.sender_address().to_string(),
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    pool_id,
                    taker_fee: move_event.taker_fee,
                    maker_fee: move_event.maker_fee,
                    stake_required: move_event.stake_required,
                }));
                info!("Observed Deepbook Trade Params Update Event {:?}", txn_data);

                txn_data
            }
            "VoteEvent" => {
                let move_event: MoveVoteEvent = bcs::from_bytes(&ev.contents)?;
                let txn_kind = tx.transaction.transaction_data().clone().into_kind();
                let first_command = txn_kind.iter_commands().next();
                let package = if let Some(Command::MoveCall(move_call)) = first_command {
                    move_call.package.to_string()
                } else {
                    "".to_string()
                };
                let mut event_digest = tx.transaction.digest().to_string();
                event_digest.push_str(&event_index.to_string());
                let txn_data = Some(ProcessedTxnData::Votes(Votes {
                    digest: tx.transaction.digest().to_string(),
                    event_digest,
                    sender: tx.transaction.sender_address().to_string(),
                    checkpoint,
                    checkpoint_timestamp_ms,
                    package,
                    pool_id: move_event.pool_id.to_string(),
                    balance_manager_id: move_event.balance_manager_id.to_string(),
                    epoch: move_event.epoch,
                    from_proposal_id: move_event.from_proposal_id.map(|id| id.to_string()),
                    to_proposal_id: move_event.to_proposal_id.to_string(),
                    stake: move_event.stake,
                }));
                info!("Observed Deepbook Vote Event {:?}", txn_data);

                txn_data
            }
            _ => {
                // todo: metrics.total_sui_bridge_txn_other.inc();
                None
            }
        }
    } else {
        None
    })
}
