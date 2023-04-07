// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useState, useCallback } from 'react';

export const useIntersectionObserver = ({
    root = null,
    rootMargin = '0px',
    threshold = 1,
} = {}) => {
    const [observer, setObserver] = useState<IntersectionObserver>();
    const [isIntersecting, setIntersecting] = useState(false);

    const containerRef = useCallback(
        (node: HTMLDivElement) => {
            if (node) {
                const observer = new IntersectionObserver(
                    ([entry]) => {
                        setIntersecting(entry.isIntersecting);
                    },
                    { root, rootMargin, threshold }
                );

                observer.observe(node);
                setObserver(observer);
            }
        },
        [root, rootMargin, threshold]
    );

    return { containerRef, isIntersecting, observer };
};
