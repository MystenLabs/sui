// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useLocation } from 'react-router-dom';

import { plausible } from '../utils/plausible';

import { useNetwork } from '~/context';

export function usePageView() {
    const [network] = useNetwork();
    const { pathname } = useLocation();

    useEffect(() => {
        // Send a pageview to Plausible
        plausible.trackPageview({
            url: pathname,
        });
        // Send a network event to Plausible with the page and url params
        plausible.trackEvent('PageByNetwork', {
            props: {
                name: network,
                source: pathname,
            },
        });
    }, [network, pathname]);
}
