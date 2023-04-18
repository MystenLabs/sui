// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useQuery } from '@tanstack/react-query';

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
    return useQuery(
        ['image-mod', url],
        async () => {
            if (!isURL || !enabled) return true;
            try {
                const resp = await fetch(`https://imgmod.sui.io/img`, {
                    method: 'POST',
                    body: JSON.stringify({ url }),
                    headers: { 'content-type': 'application/json' },
                });
                const allowed = await resp.json();
                return allowed;
            } catch (e) {
                return false;
            }
        },
        {
            placeholderData: false,
            staleTime: 24 * 60 * 60 * 1000,
            cacheTime: Infinity,
        }
    );
}
