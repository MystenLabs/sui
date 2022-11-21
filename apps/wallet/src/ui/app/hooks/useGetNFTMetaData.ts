// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getObjectFields,
    isSuiObject,
    type GetObjectDataResponse,
} from '@mysten/sui.js';
import { useQuery } from '@tanstack/react-query';
import { useMemo } from 'react';

import { api } from '../redux/store/thunk-extras';

type NFTMetadata = {
    name: string | false;
    description: string | false;
    url?: string;
} | null;

export function useGetObjectData(
    objectId: string | null
): GetObjectDataResponse | null {
    const data = useQuery(
        ['object', objectId],
        async () => {
            if (!objectId) return null;
            return api.instance.fullNode.getObject(objectId);
        },
        { enabled: !!objectId, staleTime: Infinity }
    );
    return data?.data || null;
}

export function useGetNFTMetadata(objectID: string | null): NFTMetadata {
    const data = useGetObjectData(objectID);

    const nftMeta = useMemo(() => {
        if (!data) return null;
        const { details } = data || {};
        if (!isSuiObject(details)) return null;
        const fields = getObjectFields(data);

        if (!fields?.url) return null;
        return {
            description:
                typeof fields.description === 'string' && fields.description,
            name: typeof fields.name === 'string' && fields.name,
            url: fields.url,
        };
    }, [data]);
    return nftMeta;
}
