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
        const owner = getObjectOwner(objectData) as { AddressOwner: string };
        return owner.AddressOwner === address ? objectData.details : null;
    }, [address, objectData]);
}
