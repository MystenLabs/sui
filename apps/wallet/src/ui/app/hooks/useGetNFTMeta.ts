// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getObjectFields, isSuiObject } from '@mysten/sui.js';
import { useMemo } from 'react';

import { useGetObjectData } from './useGetObjectData';

type NFTMetadata = {
    name: string | false;
    description: string | false;
    url: string;
} | null;

export function useGetNFTMeta(objectID: string): NFTMetadata {
    const { data, isError } = useGetObjectData(objectID);

    const nftMeta = useMemo(() => {
        if (isError) return null;

        const { details } = data || {};
        if (!isSuiObject(details) || !data) return null;
        const fields = getObjectFields(data);
        if (!fields?.url) return null;
        return {
            description:
                typeof fields.description === 'string' && fields.description,
            name: typeof fields.name === 'string' && fields.name,
            url: fields.url,
        };
    }, [data, isError]);

    return nftMeta;
}
