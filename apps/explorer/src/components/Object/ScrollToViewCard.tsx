// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useRef, type ReactNode } from 'react';

interface ScrollToViewCardProps {
    children: ReactNode;
    inView: boolean;
}

export function ScrollToViewCard({ children, inView }: ScrollToViewCardProps) {
    const scrollViewRef = useRef<HTMLDivElement | null>(null);

    useEffect(() => {
        if (!scrollViewRef?.current || !inView) return;

        scrollViewRef.current.scrollIntoView({
            behavior: 'smooth',
            block: 'nearest',
        });
    }, [inView, scrollViewRef]);
    return <div ref={scrollViewRef}>{children}</div>;
}
