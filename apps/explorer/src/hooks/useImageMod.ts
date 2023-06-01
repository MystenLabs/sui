// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useAppsBackend } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

export enum VISIBILITY {
    PASS = 'PASS',
    BLUR = 'BLUR',
    HIDE = 'HIDE',
}

type ImageModeration = {
    visibility?: VISIBILITY;
};

const placeholderData = {
    visibility: VISIBILITY.PASS,
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
        queryKey: ['image', url, enabled],
        queryFn: async () => {
            if (!isURL || !enabled) return placeholderData;

            return request<ImageModeration>('image', {
                url,
            });
        },
        placeholderData,
        staleTime: 24 * 60 * 60 * 1000,
        cacheTime: Infinity,
    });
}
