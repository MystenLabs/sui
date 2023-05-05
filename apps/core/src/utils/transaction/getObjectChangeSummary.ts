// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    type SuiTransactionBlockResponse,
    type SuiAddress,
    type DryRunTransactionBlockResponse,
    SuiObjectChangeTransferred,
    SuiObjectChangeCreated,
    SuiObjectChangeMutated,
    SuiObjectChangePublished,
} from '@mysten/sui.js';

export type ObjectChangeSummary = {
    mutated: SuiObjectChangeMutated[];
    created: SuiObjectChangeCreated[];
    transferred: SuiObjectChangeTransferred[];
    published: SuiObjectChangePublished[];
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
            change.type === 'created' &&
            (typeof currentAddress === 'undefined' ||
                change.sender === currentAddress)
    ) as SuiObjectChangeCreated[];

    const transferred = objectChanges.filter(
        (change) => change.type === 'transferred'
    ) as SuiObjectChangeTransferred[];

    const published = objectChanges.filter(
        (change) => change.type === 'published'
    ) as SuiObjectChangePublished[];

    return {
        mutated,
        created,
        transferred,
        published,
    };
};
