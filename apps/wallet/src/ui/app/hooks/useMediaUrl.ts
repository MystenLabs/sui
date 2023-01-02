// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject } from '@mysten/sui.js';
import { useMemo } from 'react';

import type { SuiData } from '@mysten/sui.js';

export default function useMediaUrl(objData: SuiData | null) {
    const { fields } = (isSuiMoveObject(objData) && objData) || {};
    return useMemo(() => {
        if (fields) {
            // eslint-disable-next-line @typescript-eslint/no-explicit-any
            const flds = fields as Record<string, any>;
            const mediaUrl = flds.url || flds.metadata?.fields.url;
            if (typeof mediaUrl === 'string') {
                return mediaUrl.replace(/^ipfs:\/\//, 'https://ipfs.io/ipfs/');
            }
        }
        return null;
    }, [fields]);
}
