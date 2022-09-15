// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject, getObjectId, getObjectFields } from '@mysten/sui.js';

import useFileExtentionType from './useFileExtentionType';
import useMediaUrl from './useMediaUrl';

import type { SuiObject } from '@mysten/sui.js';

export default function useNFTBasicData(nftObj: SuiObject) {
    const nftObjectID = getObjectId(nftObj.reference);
    const filePath = useMediaUrl(nftObj.data);
    const nftFields = isSuiMoveObject(nftObj.data)
        ? getObjectFields(nftObj.data)
        : null;
    const fileExtentionType = useFileExtentionType(filePath || '');
    return {
        nftObjectID,
        filePath,
        nftFields,
        fileExtentionType,
    };
}
