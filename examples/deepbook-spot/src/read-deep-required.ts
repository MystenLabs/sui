// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#deep-required
import type { DeepBookTestnetClient } from './client.js';

// How much DEEP a trade needs comes from the pool's floating DEEP price, so it
// cannot be hardcoded. `getQuantityOut` returns the base out, quote out, and the
// DEEP required for the requested quantity. Pass a nonzero base OR quote amount.
export async function readDeepRequired(
	client: DeepBookTestnetClient,
	poolKey: string,
	baseQuantity: number,
	quoteQuantity: number,
) {
	const result = await client.deepbook.getQuantityOut(poolKey, baseQuantity, quoteQuantity);
	return result; // { baseOut, quoteOut, deepRequired }
}
// docs::/#deep-required
