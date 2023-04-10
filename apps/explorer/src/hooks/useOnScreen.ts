// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, type MutableRefObject, useRef } from 'react';

export const useOnScreen = (ref: MutableRefObject<Element | null>) => {
    const [isIntersecting, setIsIntersecting] = useState(false);

    const observer = useRef(
        new IntersectionObserver(
            ([entry]) => setIsIntersecting(entry.isIntersecting),
            {
                threshold: [1],
            }
        )
    );

    useEffect(() => {
        const currObserver = observer.current;
        ref.current && currObserver.observe(ref.current);
        return () => {
            currObserver.disconnect();
        };
    });

    return { isIntersecting };
};
