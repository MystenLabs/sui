// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject } from '@mysten/sui.js';
import { useMemo } from 'react';

import type { SuiData } from '@mysten/sui.js';

export const prepareImageUrl = (mediaUrl: string) =>
    mediaUrl.replace(/^ipfs:\/\//, 'https://ipfs.io/ipfs/');

export default function useMediaUrl(objData: SuiData) {
    const { fields } = (isSuiMoveObject(objData) && objData) || {};
    return useMemo(() => {
        if (fields) {
            const mediaUrl = fields.url;
            if (typeof mediaUrl === 'string') {
                return prepareImageUrl(mediaUrl);
            }
        }
        return null;
    }, [fields]);
}
