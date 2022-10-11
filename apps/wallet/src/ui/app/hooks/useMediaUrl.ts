// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject } from '@mysten/sui.js';
import { useMemo } from 'react';

import type { SuiData } from '@mysten/sui.js';

export default function useMediaUrl(objData: SuiData) {
    const { fields } = (isSuiMoveObject(objData) && objData) || {};
    return useMemo(() => {
        if (fields) {
            const mediaUrl = fields.url || fields.metadata?.fields.url;
            if (typeof mediaUrl === 'string') {
                return mediaUrl.replace(/^ipfs:\/\//, 'https://ipfs.io/ipfs/');
            }
        }
        return null;
    }, [fields]);
}
