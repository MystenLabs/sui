// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetObject } from '@mysten/core';
import {
    is,
    SuiObjectData,
    getObjectOwner,
    type SuiAddress,
} from '@mysten/sui.js';
import { useMemo } from 'react';

import { useObjectsOwnedByAddress } from './useObjectsOwnedByAddress';

export function useOwnedNFT(
    nftObjectId: string | null,
    address: SuiAddress | null
) {
    const { data: ownedObjects } = useObjectsOwnedByAddress(address, {
        options: { showType: true, showDisplay: true },
    });
    const data = useGetObject(nftObjectId);
    const { data: objectData } = data;
    const objectDetails = useMemo(() => {
        const ownedObjectIds = ownedObjects?.map((obj) => obj.data?.objectId);
        if (!objectData || !is(objectData.data, SuiObjectData) || !address)
            return null;
        const objectOwner = getObjectOwner(objectData);
        const isOwner =
            ownedObjectIds?.includes(objectData.data.objectId) ||
            (objectOwner &&
                objectOwner !== 'Immutable' &&
                'AddressOwner' in objectOwner &&
                objectOwner.AddressOwner === address);

        return isOwner ? objectData.data : null;
    }, [address, objectData, ownedObjects]);
    return { ...data, data: objectDetails };
}
