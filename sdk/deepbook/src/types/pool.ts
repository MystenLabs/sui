// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { EventId } from '@mysten/sui.js/src/client';

export interface PoolSummary {
	poolId: string;
	baseAsset: string;
	quoteAsset: string;
}

/**
 * `next_cursor` points to the last item in the page; Reading with `next_cursor` will start from the
 * next item after `next_cursor` if `next_cursor` is `Some`, otherwise it will start from the first
 * item.
 */
export interface PaginatedPoolSummary {
	data: PoolSummary[];
	hasNextPage: boolean;
	nextCursor?: EventId | null;
}
