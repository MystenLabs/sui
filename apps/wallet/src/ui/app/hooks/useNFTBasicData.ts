// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject, getObjectId, getObjectFields } from '@mysten/sui.js';

import useFileExtensionType from './useFileExtensionType';
import useMediaUrl from './useMediaUrl';

import type { SuiObject } from '@mysten/sui.js';

export default function useNFTBasicData(nftObj: SuiObject | null) {
    const nftObjectID = (nftObj && getObjectId(nftObj.reference)) || null;
    const filePath = useMediaUrl(nftObj?.data || null);
    let objType = null;
    let nftFields = null;
    if (nftObj && isSuiMoveObject(nftObj.data)) {
        objType = nftObj.data.type;
        nftFields = getObjectFields(nftObj.data) as  // eslint-disable-next-line @typescript-eslint/no-explicit-any
            | Record<string, any>
            | undefined;
    }
    const fileExtensionType = useFileExtensionType(filePath || '');
    return {
        nftObjectID,
        filePath,
        nftFields,
        fileExtensionType,
        objType,
    };
}
