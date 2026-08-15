// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc::proto::sui::rpc::v2::Checkpoint;
use sui_rpc::proto::sui::rpc::v2::Event;
use sui_rpc::proto::sui::rpc::v2::ExecutedTransaction;
use sui_rpc::proto::sui::rpc::v2::ListCheckpointsResponse;
use sui_rpc::proto::sui::rpc::v2::ListEventsResponse;
use sui_rpc::proto::sui::rpc::v2::ListTransactionsResponse;
use sui_rpc::proto::sui::rpc::v2::QueryEnd;
use sui_rpc::proto::sui::rpc::v2::QueryEndReason;
use sui_rpc::proto::sui::rpc::v2::Watermark;
use sui_rpc_cursor::Position;

use crate::ledger_history::query_options::QueryOptions;
use crate::ledger_history::query_options::RangeExhaustion;
use crate::ledger_history::watermark::ScanTerminal;

pub trait ListResponseFrame: Default {
    type Item;
    fn set_item(&mut self, item: Self::Item);
    fn set_watermark(&mut self, watermark: Watermark);
    fn set_end(&mut self, end: QueryEnd);
}

impl ListResponseFrame for ListCheckpointsResponse {
    type Item = Checkpoint;
    fn set_item(&mut self, item: Self::Item) {
        self.checkpoint = Some(item);
    }
    fn set_watermark(&mut self, watermark: Watermark) {
        self.watermark = Some(watermark);
    }
    fn set_end(&mut self, end: QueryEnd) {
        self.end = Some(end);
    }
}

impl ListResponseFrame for ListTransactionsResponse {
    type Item = ExecutedTransaction;
    fn set_item(&mut self, item: Self::Item) {
        self.transaction = Some(item);
    }
    fn set_watermark(&mut self, watermark: Watermark) {
        self.watermark = Some(watermark);
    }
    fn set_end(&mut self, end: QueryEnd) {
        self.end = Some(end);
    }
}

impl ListResponseFrame for ListEventsResponse {
    type Item = Event;
    fn set_item(&mut self, item: Self::Item) {
        self.event = Some(item);
    }
    fn set_watermark(&mut self, watermark: Watermark) {
        self.watermark = Some(watermark);
    }
    fn set_end(&mut self, end: QueryEnd) {
        self.end = Some(end);
    }
}

pub fn item_response<F: ListResponseFrame>(item: F::Item, watermark: Watermark) -> F {
    let mut response = F::default();
    response.set_item(item);
    response.set_watermark(watermark);
    response
}

pub fn watermark_response<F: ListResponseFrame>(watermark: Watermark) -> F {
    let mut response = F::default();
    response.set_watermark(watermark);
    response
}

pub fn end_response<F: ListResponseFrame>(watermark: Watermark, reason: QueryEndReason) -> F {
    let mut end = QueryEnd::default();
    end.reason = Some(reason as i32);

    let mut response = F::default();
    response.set_watermark(watermark);
    response.set_end(end);
    response
}

/// The natural-completion terminal frame: the resolved range's exhaustion
/// rendered as watermark + end, with the reason echoed for metrics.
pub fn range_end_response<F: ListResponseFrame>(
    options: &QueryOptions,
    exhaustion: RangeExhaustion,
    position: Position,
    covered_checkpoint_bound: Option<u64>,
    interval_empty: bool,
) -> (F, QueryEndReason) {
    let terminal = ScanTerminal::from_range_exhaustion(exhaustion, position, interval_empty);
    let reason = terminal.reason();
    (
        end_response(
            terminal.into_watermark(options, covered_checkpoint_bound),
            reason,
        ),
        reason,
    )
}
