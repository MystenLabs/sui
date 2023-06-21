// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';

import { useAppsBackend } from './useAppsBackend';

type ProductAnalyticsConfigResponse = { mustProvideCookieConsent: boolean };

export function useProductAnalyticsConfig() {
	const { request } = useAppsBackend();
	return useQuery({
		queryKey: ['apps-backend', 'product-analytics-config'],
		queryFn: () => request<ProductAnalyticsConfigResponse>('product-analytics'),
		staleTime: 24 * 60 * 60 * 1000,
		cacheTime: Infinity,
	});
}
