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

export type ObjectSummaryChange =
    | SuiObjectChangeMutated
    | SuiObjectChangeCreated
    | SuiObjectChangeTransferred;

export type ObjectSummaryChangeWithNFT<T> = T & {
    locationIdType: LocationIdType;
    nftMeta?: Record<string, string | null>;
};

export function getGroupByOwner(objectSummaryChanges: ObjectSummaryChange[]) {
    if (!objectSummaryChanges) {
        return {};
    }

    return objectSummaryChanges.reduce(
        (
            mapByOwner: Record<
                string,
                ObjectSummaryChangeWithNFT<ObjectSummaryChange>[]
            >,
            change
        ) => {
            const owner = 'owner' in change ? change.owner : null;

            if (!owner) {
                return mapByOwner;
            }

            let key;
            let locationIdType;
            if (owner !== 'Immutable' && 'AddressOwner' in owner) {
                key = owner.AddressOwner;
                locationIdType = LocationIdType.AddressOwner;
            } else if (owner !== 'Immutable' && 'ObjectOwner' in owner) {
                key = owner.ObjectOwner;
                locationIdType = LocationIdType.ObjectOwner;
            } else if (owner !== 'Immutable' && 'Shared' in owner) {
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
