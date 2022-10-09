// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo } from 'react';

interface Props {
    target: Element | null;
    callback: MutationCallback;
}

export const useWaitForElement = ({ target, callback }: Props): void => {
    const observer = useMemo(() => new MutationObserver(callback), [callback]);

    useEffect(() => {
        if (observer && target) {
            observer.observe(document.documentElement, {
                childList: true,
                subtree: true,
            });
            return () => observer.disconnect();
        }
    }, [target, observer]);
};
