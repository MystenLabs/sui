// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    type SuiTransactionBlockResponse,
    type SuiAddress,
    type DryRunTransactionBlockResponse,
    SuiObjectChangeTransferred,
    SuiObjectChangeCreated,
    SuiObjectChangeMutated,
} from '@mysten/sui.js';

export type ObjectChangeSummary = {
    mutated: SuiObjectChangeMutated[];
    created: SuiObjectChangeCreated[];
    transferred: SuiObjectChangeTransferred[];
};

export const getObjectChangeSummary = (
    transaction: DryRunTransactionBlockResponse | SuiTransactionBlockResponse,
    currentAddress?: SuiAddress | null
) => {
    const { objectChanges } = transaction;
    if (!objectChanges) return null;

    const mutated = objectChanges.filter(
        (change) => change.type === 'mutated'
    ) as SuiObjectChangeMutated[];

    const created = objectChanges.filter(
        (change) =>
            change.type === 'created' && change.sender === currentAddress
    ) as SuiObjectChangeCreated[];

    const transferred = objectChanges.filter(
        (change) => change.type === 'transferred'
    ) as SuiObjectChangeTransferred[];

    return {
        mutated,
        created,
        transferred,
    };
};
