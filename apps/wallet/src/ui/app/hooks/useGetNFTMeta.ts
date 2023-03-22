// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { is, SuiObjectData } from '@mysten/sui.js';
import { useMemo } from 'react';

import { useGetObject } from './useGetObject';

export type LinkData = {
    href: string;
    display: string;
};

function toLinkData(link: string): LinkData | string | null {
    try {
        const url = new URL(link);
        return { href: link, display: url.hostname };
    } catch (e) {
        return link || null;
    }
}

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
        const { name, description, creator, img_url, link, project_url } =
            data.display;
        return {
            name: name || null,
            description: description || null,
            imageUrl: img_url || null,
            link: link ? toLinkData(link) : null,
            projectUrl: project_url ? toLinkData(project_url) : null,
            creator: creator ? toLinkData(creator) : null,
        };
    }, [resp]);
    return {
        ...resp,
        data: nftMeta,
    };
}
