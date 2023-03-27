// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_SYSTEM_STATE_OBJECT_ID, TransactionBlock } from '@mysten/sui.js';

export function createStakeTransaction(amount: bigint, validator: string) {
    const tx = new TransactionBlock();
    const stakeCoin = tx.splitCoins(tx.gas, [tx.pure(amount)]);
    tx.moveCall({
        target: '0x3::sui_system::request_add_stake',
        arguments: [
            tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
            stakeCoin,
            tx.pure(validator),
        ],
    });
    return tx;
}

export function createUnstakeTransaction(stakedSuiId: string) {
    const tx = new TransactionBlock();
    tx.moveCall({
        target: '0x3::sui_system::request_withdraw_stake',
        arguments: [
            tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
            tx.object(stakedSuiId),
        ],
    });
    return tx;
}
