// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getTransactionKindName, getMoveCallTransaction } from '@mysten/sui.js';

import type { SuiTransactionResponse } from '@mysten/sui.js';

export function checkStakingTxn(txn: SuiTransactionResponse) {
    const { certificate } = txn;
    const txnKind = getTransactionKindName(certificate.data.transactions[0]);

    if (txnKind !== 'Call') return null;

    const moveCallTxn = getMoveCallTransaction(
        certificate.data.transactions[0]
    );
    if (
        moveCallTxn?.module === 'sui_system' &&
        moveCallTxn?.function === 'request_add_delegation_mul_coin'
    )
        return 'Staked';
    if (
        moveCallTxn?.module === 'sui_system' &&
        moveCallTxn?.function === 'request_withdraw_delegation'
    )
        return 'Unstaked';
    return null;
}
