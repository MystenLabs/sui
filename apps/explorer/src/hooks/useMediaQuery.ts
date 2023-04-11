// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback, useLayoutEffect, useState } from 'react';

export function useMediaQuery(query: string): boolean {
    const getMatchMedia = (query: string): MediaQueryList | null => {
        // Prevents SSR issues
        if (typeof window !== 'undefined') {
            return window.matchMedia(query);
        }

        return null;
    };

    const getMatches = useCallback(
        (query: string): boolean => getMatchMedia(query)?.matches ?? false,
        []
    );

    const [matches, setMatches] = useState<boolean>(getMatches(query));

    useLayoutEffect(() => {
        const matchMedia = getMatchMedia(query);
        const listener = () => setMatches(getMatches(query));

        listener();

        if (matchMedia) {
            matchMedia.addEventListener('change', listener);
        }

        return () => {
            if (matchMedia) {
                matchMedia.removeEventListener('change', listener);
            }
        };
    }, [getMatches, query]);

    return matches;
}
