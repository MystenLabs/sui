// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useLocation } from 'react-router-dom';
import Browser from 'webextension-polyfill';

import { trackPageview, trackEvent } from '../plausible';
import { useAppSelector } from '_hooks';
import { growthbook } from '_src/ui/app/experimentation/feature-gating';

export const MAIN_UI_URL = Browser.runtime.getURL('ui.html');
const WALLET_VERSION = Browser.runtime.getManifest().version;

export function openInNewTab() {
    return Browser.tabs.create({ url: MAIN_UI_URL });
}

export function usePageView() {
    const location = useLocation();
    const { apiEnv, customRPC } = useAppSelector((state) => state.app);
    // Use customRPC url if apiEnv is customRPC
    const activeNetwork =
        customRPC && apiEnv === 'customRPC' ? customRPC : apiEnv.toUpperCase();

    useEffect(() => {
        // NOTE: This is a hack to work around hook timing issues with the Growthbook SDK.
        // Issue: https://github.com/growthbook/growthbook/issues/915
        setTimeout(() => {
            growthbook.setAttributes({
                network: activeNetwork,
                version: WALLET_VERSION,
                beta: process.env.WALLET_BETA || false,
            });
        }, 0);

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
    }, [activeNetwork, location]);
}
