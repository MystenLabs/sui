// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useCallback } from 'react';

const backendUrl =
	process.env.NODE_ENV === 'development' ? 'http://localhost:3003' : 'https://apps-backend.sui.io';

export function useAppsBackend() {
	const request = useCallback(
		async <T>(
			path: string,
			queryParams?: Record<string, any>,
			options?: RequestInit,
		): Promise<T> => {
			const res = await fetch(formatRequestURL(`${backendUrl}/${path}`, queryParams), options);

			if (!res.ok) {
				throw new Error('Unexpected response');
			}

			return res.json();
		},
		[],
	);

	return { request };
}

function formatRequestURL(url: string, queryParams?: Record<string, any>) {
	if (queryParams && Object.keys(queryParams).length > 0) {
		const searchParams = new URLSearchParams(queryParams);
		return `${url}?${searchParams}`;
	}
	return url;
}
