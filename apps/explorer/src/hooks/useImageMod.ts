// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAppsBackend } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

export enum VISIBILITY {
    PASS = 'PASS',
    HIDE = 'HIDE',
    SHOW = 'SHOW',
}

type ImageMod = {
    pass: VISIBILITY;
};

const isURL = (url?: string) => {
    if (!url) return false;
    try {
        new URL(url);
        return true;
    } catch (e) {
        return false;
    }
};

export function useImageMod({
    url,
    enabled = true,
}: {
    url?: string;
    enabled?: boolean;
}) {
    const { request } = useAppsBackend();
    return useQuery({
        queryKey: ['image-mod', url, enabled],
        queryFn: async () => {
            if (!isURL(url)) {
                return {
                    pass: 'PASS',
                } as ImageMod;
            }
            return request<ImageMod>(`/image${url}`, {});
        },
        cacheTime: 24 * 60 * 60 * 1000,
        staleTime: Infinity,
    });
}
