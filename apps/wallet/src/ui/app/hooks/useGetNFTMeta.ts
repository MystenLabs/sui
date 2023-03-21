// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { processDisplay } from '@mysten/core';
import { is, SuiObjectData } from '@mysten/sui.js';
import { useMemo } from 'react';

import { useGetObject } from './useGetObject';

export type NFTMetadata = {
    name: string | null;
    description: string | null;
    url: string;
};

export function useGetNFTMeta(objectID: string) {
    const resp = useGetObject(objectID, { showDisplay: true });
    const nftMeta = useMemo(() => {
        if (!resp.data) return null;
        const { data } = resp.data || {};
        if (!is(data, SuiObjectData) || !data.display) return null;
        return processDisplay(data.display);
    }, [resp]);
    return {
        ...resp,
        data: nftMeta,
    };
}
