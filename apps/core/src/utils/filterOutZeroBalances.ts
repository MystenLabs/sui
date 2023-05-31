// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type CoinBalance } from '@mysten/sui.js';

// Remove tokens with zero balance and sort by balance
export function filterOutZeroBalances(tokens: CoinBalance[]) {
    return tokens.filter((token) => Number(token.totalBalance) > 0);
}
