// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useQuery } from '@tanstack/react-query';

const isURL = (url: string) => {
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
                const allowed = await (
                    await fetch(`https://imgmod.sui.io/img`, {
                        method: 'POST',
                        headers: { 'content-type': 'application/json' },
                        body: JSON.stringify({ url }),
                    })
                ).json();
                return allowed;
            } catch (e) {
                return false;
            }
        },
        {
            placeholderData: false,
            staleTime: Infinity,
            cacheTime: Infinity,
        }
    );
}
