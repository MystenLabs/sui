// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useLocation } from 'react-router-dom';

import { ampli } from '~/utils/analytics/ampli';

export function useInitialPageView(activeNetwork: string) {
	const location = useLocation();

	// Set user properties for the user's page information
	useEffect(() => {
		ampli.identify(undefined, {
			pageDomain: window.location.hostname,
			pagePath: location.pathname,
			pageUrl: window.location.href,
			activeNetwork,
		});
	}, [location.pathname, activeNetwork]);

	// Log an initial page view event
	useEffect(() => {
		ampli.openedSuiExplorer();
	}, []);
}
