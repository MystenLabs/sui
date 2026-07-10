// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_rpc::proto::sui::rpc::v2alpha::QueryEndReason;

/// Final reason for a successful query stream. Hitting the requested item limit
/// takes precedence over the range's natural end reason: when `emitted` reaches
/// the limit the stream stopped early and more data may exist. On `ItemLimit`
/// the `QueryEnd` rides the final item frame itself; otherwise it arrives on a
/// payload-free final frame whose watermark, when present, is a claim strictly
/// newer than anything already emitted.
pub(super) fn query_end(
    emitted: usize,
    limit_items: usize,
    end_reason: QueryEndReason,
) -> QueryEndReason {
    if emitted == limit_items {
        QueryEndReason::ItemLimit
    } else {
        end_reason
    }
}
