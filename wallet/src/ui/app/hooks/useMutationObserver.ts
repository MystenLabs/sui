// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect, useState } from 'react';

import type { RefObject } from 'react';

interface Props {
    target: RefObject<Element> | Element | Node | null;
    options: MutationObserverInit;
    callback: MutationCallback;
}

const getRefElement = <T>(
    element?: RefObject<Element> | T
): Element | T | undefined | null => {
    if (element && 'current' in element) {
        return element.current;
    }

    return element;
};

const useMutationObserver = ({
    target,
    options = {},
    callback,
}: Props): void => {
    const [observer, setObserver] = useState<MutationObserver | null>(null);

    useEffect(() => setObserver(new MutationObserver(callback)), [callback]);

    useEffect(() => {
        const element = getRefElement(target);
        if (observer && element) {
            observer.observe(document.documentElement, options);
            return () => observer.disconnect();
        }
    }, [target, observer, options]);
};

export default useMutationObserver;
