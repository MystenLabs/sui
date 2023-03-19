// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiTransactionResponse } from '@mysten/sui.js';

// Get the amount from events and transfer data
export function getAmount(txnData: SuiTransactionResponse) {
    const { balanceChanges } = txnData;
    const transfer = balanceChanges?.map(({ coinType, owner, amount }) => {
        const addressOwner =
            owner !== 'Immutable' && 'AddressOwner' in owner
                ? owner.AddressOwner
                : null;
        const ownerAddress =
            owner !== 'Immutable' && 'ObjectOwner' in owner
                ? owner.ObjectOwner
                : null;

        return {
            coinType,
            address: addressOwner || ownerAddress,
            amount: +amount,
        };
    });
    return transfer;
}
