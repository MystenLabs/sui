// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useLocation } from 'react-router-dom';
import Browser from 'webextension-polyfill';

import { trackPageview } from '../plausible';

export function openInNewTab() {
    const url = Browser.runtime.getURL('ui.html');
    return Browser.tabs.create({ url });
}

export function usePageView() {
    const location = useLocation();
    useEffect(() => {
        trackPageview({
            url: location.pathname,
        });
    }, [location]);
}
