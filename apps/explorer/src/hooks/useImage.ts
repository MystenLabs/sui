// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useEffect, useRef, useState } from 'react';

import { useImageMod } from './useImageMod';

type Status = 'loading' | 'failed' | 'loaded';

interface UseImageProps {
    src: string;
    moderate?: boolean;
}

export function useImage({ src, moderate = false }: UseImageProps) {
    const [status, setStatus] = useState<Status>('loading');
    const formatted = src.replace(/^ipfs:\/\//, 'https://ipfs.io/ipfs/');
    const { data: allowed, isFetched } = useImageMod({ url: formatted });
    const ref = useRef<HTMLImageElement | null>(null);

    useEffect(() => {
        const img = new globalThis.Image();
        ref.current = img;
        img.src = src;
        img.onload = () => !moderate && setStatus('loaded');
        img.onerror = () => setStatus('failed');
    });

    useEffect(() => {
        if (moderate && isFetched) setStatus('loaded');
    }, [moderate, isFetched]);

    return { url: formatted, nsfw: moderate && !allowed, status };
}

export default useImage;
