// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useCallback, useLayoutEffect, useRef, useState } from 'react';

import { useImageMod } from './useImageMod';

type Status = 'loading' | 'failed' | 'loaded';

interface UseImageProps {
    src?: string;
    moderate?: boolean;
}

export function useImage({ src = '', moderate = true }: UseImageProps) {
    const [status, setStatus] = useState<Status>('loading');
    const formatted = src?.replace(/^ipfs:\/\//, 'https://ipfs.io/ipfs/');
    const { data: allowed, isFetched } = useImageMod({
        url: formatted,
        enabled: moderate,
    });

    const ref = useRef<HTMLImageElement | null>(null);

    const cleanup = () => {
        if (ref.current) {
            ref.current.onload = null;
            ref.current.onerror = null;
            ref.current = null;
        }
    };

    const load = useCallback(() => {
        if (!src) setStatus('failed');
        const img = new Image();
        img.src = formatted;

        img.onload = () =>
            (!moderate || (moderate && isFetched)) && setStatus('loaded');
        img.onerror = () => setStatus('failed');
        ref.current = img;
    }, [src, formatted, moderate, isFetched]);

    useLayoutEffect(() => {
        load();
        return () => cleanup();
    }, [load]);

    return { nsfw: !allowed, url: formatted, status, ref };
}

export default useImage;
