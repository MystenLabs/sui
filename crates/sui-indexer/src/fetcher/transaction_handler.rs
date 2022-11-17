// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_json_rpc_types::TransactionsPage;
use sui_sdk::SuiClient;
use sui_types::base_types::TransactionDigest;
use sui_types::query::TransactionQuery;
use tracing::{info, warn};

// TODO: want to tune the size
const TRANSACTION_PAGE_SIZE: usize = 100;

pub struct TransactionHandler {
    rpc_client: SuiClient,
}

impl TransactionHandler {
    pub fn new(rpc_client: SuiClient) -> Self {
        Self { rpc_client }
    }

    pub async fn run_forever(&self) {
        // TODO: read next cursor from DB if available
        let mut next_cursor = None;
        loop {
            let page = self.get_transaction_page(next_cursor).await;
            if let Ok(page) = page {
                info!("Current transaction page {:?}", page.data);
                // TODO: write this page of txns to DB
                next_cursor = page.next_cursor;
            } else {
                warn!("Get transaction page failed: {:?}", page);
                break;
            }
        }
    }

    async fn get_transaction_page(
        &self,
        cursor: Option<TransactionDigest>,
    ) -> anyhow::Result<TransactionsPage> {
        self.rpc_client
            .read_api()
            .get_transactions(
                TransactionQuery::All,
                cursor,
                Some(TRANSACTION_PAGE_SIZE),
                None,
            )
            .await
    }
}
