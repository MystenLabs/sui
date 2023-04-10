// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, type MutableRefObject } from 'react';

export const useOnScreen = (ref: MutableRefObject<Element | null>) => {
    const [isIntersecting, setIsIntersecting] = useState(false);

    const observer = new IntersectionObserver(
        ([entry]) => setIsIntersecting(entry.isIntersecting),
        {
            threshold: [1],
        }
    );

    useEffect(() => {
        ref.current && observer.observe(ref.current);
        return () => {
            observer.disconnect();
        };
    });

    return { isIntersecting };
};
