// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObjectData } from '@mysten/sui.js';
import { useMemo } from 'react';

import { useGetObject } from './useGetObject';

export type NFTMetadata = {
    name: string | null;
    description: string | null;
    url: string;
};

export function useGetNFTMeta(objectID: string) {
    const resp = useGetObject(objectID);
    const nftMeta = useMemo(() => {
        if (!resp.data) return null;
        const { data } = resp.data || {};
        if (!is(data, SuiObjectData) || !data.display) return null;
        const { name, description, creator, image_url, link, project_url } =
            data.display;
        return {
            name: name || null,
            description: description || null,
            imageUrl: image_url || null,
            link: link || null,
            projectUrl: project_url || null,
            creator: creator || null,
        };
    }, [resp]);
    return {
        ...resp,
        data: nftMeta,
    };
}
