// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    SuiObjectChangeCreated,
    SuiObjectChangeMutated,
    SuiObjectChangeTransferred,
} from '@mysten/sui.js';

export enum LocationIdType {
    AddressOwner = 'AddressOwner',
    ObjectOwner = 'ObjectOwner',
    Shared = 'Shared',
    Unknown = 'Unknown',
}

type ObjectSummaryChange =
    | SuiObjectChangeMutated
    | SuiObjectChangeCreated
    | SuiObjectChangeTransferred;

export function getGroupByOwner(objectSummaryChanges: ObjectSummaryChange[]) {
    if (!objectSummaryChanges) {
        return {};
    }

    return objectSummaryChanges.reduce(
        (
            mapByOwner: Record<
                string,
                ObjectSummaryChange & { locationIdType: string }[]
            >,
            change
        ) => {
            const owner = 'owner' in change ? change.owner : {};

            let key;
            let locationIdType;
            if ('AddressOwner' in owner) {
                key = owner.AddressOwner;
                locationIdType = LocationIdType.AddressOwner;
            } else if ('ObjectOwner' in owner) {
                key = owner.ObjectOwner;
                locationIdType = LocationIdType.ObjectOwner;
            } else if ('Shared' in owner) {
                key = change.objectId;
                locationIdType = LocationIdType.Shared;
            } else {
                key = '';
                locationIdType = LocationIdType.Unknown;
            }

            mapByOwner[key as string] = mapByOwner[key as string] || [];
            mapByOwner[key as string].push({
                ...change,
                locationIdType,
            });

            return mapByOwner;
        },
        {}
    );
}
