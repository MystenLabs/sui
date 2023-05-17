// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetObject, useGetObjects } from './useGetObject';
import { useMemo } from 'react';
import { getObjectDisplay } from '@mysten/sui.js';

const getNFTMeta = ({
    name,
    description,
    creator,
    image_url,
    link,
    project_url,
}: Record<string, string>) => {
    return {
        name: name || null,
        description: description || null,
        imageUrl: image_url || null,
        link: link || null,
        projectUrl: project_url || null,
        creator: creator || null,
    };
};

export function useGetNFTMeta(objectID: string) {
    const resp = useGetObject(objectID);
    const nftMeta = useMemo(() => {
        if (!resp.data) return null;
        const display = getObjectDisplay(resp.data);
        if (!display.data) {
            return null;
        }

        return getNFTMeta(display.data);
    }, [resp]);
    return {
        ...resp,
        data: nftMeta,
    };
}

export function useNFTsMeta(objectIds: string[]) {
    const resp = useGetObjects(objectIds);

    const nftsMeta = useMemo(() => {
        if (!resp.data) return null;
        const result: Record<string, string | null>[] = [];
        const nftsObjectIds: string[] = [];

        resp.data.forEach((obj, index) => {
            const display = getObjectDisplay(obj);

            if (display.data) {
                result.push(getNFTMeta(display.data));
                nftsObjectIds.push(objectIds[index]);
            }
        });

        return {
            result,
            nftsObjectIds,
        };
    }, [objectIds, resp.data]);

    return {
        ...resp,
        data: {
            metaData: nftsMeta?.result || [],
            objectIds: nftsMeta?.nftsObjectIds || [],
        },
    };
}
