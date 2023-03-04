// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    hasPublicTransfer,
    is,
    SuiObject,
    getObjectOwner,
    type SuiAddress,
    type GetObjectDataResponse,
} from '@mysten/sui.js';
import { useMemo } from 'react';

export function useOwnedNFT(
    objectData: GetObjectDataResponse | null,
    address: SuiAddress | null
) {
    return useMemo(() => {
        if (
            !objectData ||
            !is(objectData.details, SuiObject) ||
            !hasPublicTransfer(objectData.details)
        )
            return null;
        const objectOwner = getObjectOwner(objectData);
        const owner =
            objectOwner &&
            objectOwner !== 'Immutable' &&
            'AddressOwner' in objectOwner &&
            objectOwner.AddressOwner === address
                ? objectData.details
                : null;

        return owner;
    }, [address, objectData]);
}
