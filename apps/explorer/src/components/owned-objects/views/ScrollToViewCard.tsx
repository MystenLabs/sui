// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, type ReactNode } from 'react';

interface ScrollToViewCardProps {
    children: ReactNode;
    strollTo: boolean;
}

export function ScrollToViewCard({
    children,
    strollTo,
}: ScrollToViewCardProps) {
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
