// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type SuiTransactionResponse,
    getTotalGasUsed,
    SUI_TYPE_ARG,
} from '@mysten/sui.js';

// Get the amount from events and transfer data
export function getAmount(txnData: SuiTransactionResponse) {
    const { balanceChanges } = txnData;
    const totalGasUsed = getTotalGasUsed(txnData);
    //TODO: verify this is correct
    // Only subtract gas if it is greater than 0
    const gas = totalGasUsed && totalGasUsed > 0 ? totalGasUsed : 0;
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
            // remove gas from sender amount
            // negative amount is sender
            // Remove gas from sender amount
            amount:
                coinType === SUI_TYPE_ARG && +amount < 0
                    ? +amount + (gas || 0)
                    : +amount,
        };
    });
    return transfer;
}
