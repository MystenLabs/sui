// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
// eslint-disable-next-line no-restricted-imports
import { useLocation, useSearchParams } from 'react-router-dom';

import { DEFAULT_NETWORK } from '../utils/envUtil';
import { plausible } from '../utils/plausible';

export function usePageView() {
    const { pathname } = useLocation();
    const [searchParams] = useSearchParams();

    const networkParam = searchParams.get('network');
    const network = (networkParam || DEFAULT_NETWORK).toUpperCase();

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
