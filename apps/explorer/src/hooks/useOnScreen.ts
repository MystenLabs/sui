// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, type MutableRefObject, useRef } from 'react';

export const useOnScreen = (ref: MutableRefObject<Element | null>) => {
    const [isIntersecting, setIsIntersecting] = useState(false);
    const observer = new IntersectionObserver(
        ([entry]) => setIsIntersecting(entry.isIntersecting),
        {
            threshold: [1],
        }
    );

    const observerRef = useRef(observer)

    useEffect(() => {
        ref.current && observerRef.current.observe(ref.current);
        return () => {
            observer.disconnect();
        };
    }), [];

    return { isIntersecting };
};
