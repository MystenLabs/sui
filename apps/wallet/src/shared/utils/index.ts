// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useLocation } from 'react-router-dom';
import Browser from 'webextension-polyfill';

import { WALLET_URL, plausible } from '_shared/constants';

export function openInNewTab() {
    const url = Browser.runtime.getURL('ui.html');
    return Browser.tabs.create({ url });
}

export function usePageView() {
    const location = useLocation();
    useEffect(() => {
        if (process.env.NODE_ENV !== 'development') {
            plausible.trackPageview({
                url: WALLET_URL + location.pathname,
            });
        }
    }, [location]);
}
