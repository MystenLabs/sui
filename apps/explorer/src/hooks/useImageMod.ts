// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useQuery } from '@tanstack/react-query';

export function useImageMod({ url }: { url?: string }) {
    return useQuery(
        ['image-mod', url],
        async () => {
            if (import.meta.env.DEV) return true;
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
        { enabled: !!url, staleTime: Infinity, cacheTime: Infinity }
    );
}
