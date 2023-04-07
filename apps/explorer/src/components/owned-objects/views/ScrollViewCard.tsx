// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, type ReactNode } from 'react';

interface ScrollViewCardProps {
    children: ReactNode;
    strollTo: boolean;
}

export function ScrollViewCard({ children, strollTo }: ScrollViewCardProps) {
    const scrollViewRef = useRef<HTMLDivElement | null>(null);

    useEffect(() => {
        if (!scrollViewRef?.current || !strollTo) return;

        scrollViewRef.current.scrollIntoView({
            behavior: 'smooth',
            block: 'nearest',
        });
    }, [strollTo, scrollViewRef]);
    return <div ref={scrollViewRef}>{children}</div>;
}
