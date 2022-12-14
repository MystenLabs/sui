// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getMoveCallTransaction } from '@mysten/sui.js';

import type { SuiTransactionKind } from '@mysten/sui.js';

const stakingCalls = [
    'request_add_delegation',
    'request_add_stake_with_locked_coin',
    'request_withdraw_stake',
    'request_add_delegation_with_locked_coin',
    'request_withdraw_delegation',
    'request_switch_delegation',
];

// Get known native move function
export function getMoveCallMeta(txDetails: SuiTransactionKind): {
    label: string;
    fnName: string;
    validatorAddress?: string | null;
} | null {
    const moveCall = getMoveCallTransaction(txDetails);
    if (!moveCall) return null;

    let label = 'Move Call';
    let validatorAddress;
    const fnName = moveCall.function.replace(/_/g, ' ');

    if (moveCall.module === 'devnet_nft' && moveCall.function === 'mint') {
        label = 'Minted';
    }

    if (
        moveCall.module === 'sui_system' &&
        stakingCalls.includes(moveCall.function) &&
        moveCall.arguments?.[0] === '0x5'
    ) {
        // TODO properly label staking types. For now limit to Staked and Unstaked
        label =
            moveCall.function === 'request_add_delegation'
                ? 'Staked'
                : moveCall.function === 'request_withdraw_delegation'
                ? 'Unstaked!'
                : fnName;

        validatorAddress = moveCall.arguments?.[2] as string;
    }
    return {
        label,
        fnName,
        validatorAddress,
    };
}
