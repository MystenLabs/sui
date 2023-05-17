// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import {
    type SuiAddress,
    SuiObjectChangeTransferred,
    SuiObjectChangeCreated,
    SuiObjectChangeMutated,
    SuiObjectChangePublished,
    SuiObjectChange,
    SuiObjectChangeTypes,
    DisplayFieldsResponse,
    SuiObjectChangeDeleted,
    SuiObjectChangeWrapped,
} from '@mysten/sui.js';
import { groupByOwner } from './groupByOwner';

export type WithDisplayFields<T> = T & { display?: DisplayFieldsResponse };

export type SuiObjectChangeWithDisplay = WithDisplayFields<SuiObjectChange>;

export type ObjectChangeSummary = {
    [K in SuiObjectChangeTypes]: Record<
        SuiObjectChangeTypes,
        SuiObjectChangeWithDisplay[]
    >;
};

export const getObjectChangeSummary = (
    objectChanges: SuiObjectChangeWithDisplay[],
    currentAddress?: SuiAddress | null
) => {
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

    const wrapped = objectChanges.filter(
        (change) => change.type === 'wrapped'
    ) as SuiObjectChangeWrapped[];

    const deleted = objectChanges.filter(
        (change) => change.type === 'deleted'
    ) as SuiObjectChangeDeleted[];

    return {
        mutated: groupByOwner(mutated),
        created: groupByOwner(created),
        transferred: groupByOwner(transferred),
        published: groupByOwner(published),
        wrapped: groupByOwner(wrapped),
        deleted: groupByOwner(deleted),
    };
};
