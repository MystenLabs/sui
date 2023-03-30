// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { usePostHog } from 'posthog-js/react';
import { useEffect } from 'react';
import { useLocation } from 'react-router-dom';

import { plausible } from '../utils/plausible';

import { useNetwork } from '~/context';

export function usePageView() {
    const [network] = useNetwork();
    const { pathname } = useLocation();
    const postHog = usePostHog();

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

        postHog?.capture('$pageview', { url: pathname, name: network });
    }, [network, pathname, postHog]);
}
