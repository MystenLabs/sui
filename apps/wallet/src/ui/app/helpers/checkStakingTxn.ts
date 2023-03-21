// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiTransactionResponse } from '@mysten/sui.js';

// TODO: Support programmable transactions:
export function checkStakingTxn(_txn: SuiTransactionResponse) {
    return false;
}
