// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useAppsBackend } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

// https://cloud.google.com/vision/docs/supported-files
const SUPPORTED_IMG_TYPES = [
    'image/jpeg',
    'image/png',
    'image/gif',
    'image/bmp',
    'image/webp',
    'image/x-icon',
    'application/pdf',
    'image/tiff',
];

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
        queryKey: ['image-mod', url, enabled],
        queryFn: async () => {
            if (!isURL || !enabled) return placeholderData;

            const res = await fetch(`${url}`, {
                method: 'HEAD',
            });

            const contentType = res.headers.get('Content-Type');

            if (contentType && SUPPORTED_IMG_TYPES.includes(contentType)) {
                return request<ImageModeration>('image', {
                    url,
                });
            }
        },
        placeholderData,
        staleTime: 24 * 60 * 60 * 1000,
        cacheTime: Infinity,
    });
}

// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// import { useCallback } from 'react';

// const backendUrl =
//     process.env.NODE_ENV !== 'development'
//         ? 'http://localhost:3003'
//         : 'https://apps-backend.sui.io';

// export function useAppsBackend() {
//     const request = useCallback(
//         async <T>(
//             path: string,
//             queryParams?: Record<string, any>,
//             options?: RequestInit
//         ): Promise<T> => {
//             const res = await fetch(
//                 formatRequestURL(`${backendUrl}/${path}`, queryParams),
//                 options
//             );

//             if (!res.ok) {
//                 throw new Error('Unexpected response');
//             }

//             return res.json();
//         },
//         []
//     );

//     return { request };
// }

// function formatRequestURL(url: string, queryParams?: Record<string, any>) {
//     if (queryParams && Object.keys(queryParams).length > 0) {
//         const searchParams = new URLSearchParams(queryParams);
//         return `${url}?${searchParams}`;
//     }
//     return url;
// }
