// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject, getObjectId, getObjectFields } from '@mysten/sui.js';
import { isArtNft, parseNftdata } from '@originbyte/js-sdk';

import useFileExtentionType from './useFileExtentionType';
import useMediaUrl, { prepareImageUrl } from './useMediaUrl';

import type { SuiObject } from '@mysten/sui.js';

export default function useNFTBasicData(nftObj: SuiObject) {
    const nftObjectID = getObjectId(nftObj.reference);
    let filePath = useMediaUrl(nftObj.data);
    let objType = null;
    let nftFields = null;
    if (isArtNft(nftObj)) {
        objType = nftObj.data.type;
        console.log('nftObj.data', nftObj.data);
        nftFields = parseNftdata(nftObj.data.fields);
        filePath = prepareImageUrl(nftFields.url);
        console.log('result:', nftFields, nftFields, filePath);
    } else if (isSuiMoveObject(nftObj.data)) {
        objType = nftObj.data.type;
        nftFields = getObjectFields(nftObj.data);
    }
    const fileExtentionType = useFileExtentionType(filePath || '');
    return {
        nftObjectID,
        filePath,
        nftFields,
        fileExtentionType,
        objType,
    };
}
