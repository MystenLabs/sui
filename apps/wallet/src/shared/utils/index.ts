// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useLocation } from 'react-router-dom';
import Browser from 'webextension-polyfill';

import { trackPageview } from '../plausible';

export const MAIN_UI_URL = Browser.runtime.getURL('ui.html');

export function openInNewTab() {
    return Browser.tabs.create({ url: MAIN_UI_URL });
}

export function usePageView() {
    const location = useLocation();
    useEffect(() => {
        trackPageview({
            url: location.pathname,
        });
    }, [location]);
}
