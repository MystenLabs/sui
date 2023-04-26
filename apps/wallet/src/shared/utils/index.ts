// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGrowthBook } from '@growthbook/growthbook-react';
import { useEffect } from 'react';
import { useLocation } from 'react-router-dom';
import Browser from 'webextension-polyfill';

import { trackPageview, trackEvent } from '../plausible';
import { useAppSelector } from '_hooks';
import { setAttributes } from '_src/shared/experimentation/features';

export const MAIN_UI_URL = Browser.runtime.getURL('ui.html');

export function openInNewTab() {
    return Browser.tabs.create({ url: MAIN_UI_URL });
}

export function usePageView() {
    const location = useLocation();
    const { apiEnv, customRPC } = useAppSelector((state) => state.app);
    // Use customRPC url if apiEnv is customRPC
    const activeNetwork =
        customRPC && apiEnv === 'customRPC' ? customRPC : apiEnv.toUpperCase();
    const growthBook = useGrowthBook();

    useEffect(() => {
        if (growthBook) {
            setAttributes({ apiEnv, customRPC });
        }

        trackPageview({
            url: location.pathname,
        });
        // Send a network event to Plausible with the page and url params
        trackEvent('PageByNetwork', {
            props: {
                name: activeNetwork,
                source: `${location.pathname}${location.search}`,
            },
        });
    }, [activeNetwork, location, growthBook, apiEnv, customRPC]);
}

export function isValidUrl(url: string | null) {
    if (!url) {
        return false;
    }
    try {
        new URL(url);
        return true;
    } catch (e) {
        return false;
    }
}

export function prepareLinkToCompare(link: string) {
    let adjLink = link.toLowerCase();
    if (!adjLink.endsWith('/')) {
        adjLink += '/';
    }
    return adjLink;
}
