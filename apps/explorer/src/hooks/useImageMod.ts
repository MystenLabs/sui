// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAppsBackend } from '@mysten/core';
import { useQuery } from '@tanstack/react-query';

// https://cloud.google.com/vision/docs/supported-files
const SUPPORTED_IMG_TYPES = [
	'image/jpeg',
	'image/png',
	'image/gif',
	'image/bmp',
	'image/webp',
	'image/x-icon',
	'application/pdf',
	'image/tiff',
];

export enum VISIBILITY {
	PASS = 'PASS',
	BLUR = 'BLUR',
	HIDE = 'HIDE',
}

type ImageModeration = {
	visibility?: VISIBILITY;
};

const placeholderData = {
	visibility: VISIBILITY.PASS,
};

const isURL = (url?: string) => {
	if (!url) return false;
	try {
		new URL(url);
		return true;
	} catch (e) {
		return false;
	}
};

export function useImageMod({ url = '', enabled = true }: { url?: string; enabled?: boolean }) {
	const { request } = useAppsBackend();

	return useQuery({
		queryKey: ['image-mod', url, enabled],
		queryFn: async () => {
			if (!isURL(url) || !enabled) return placeholderData;

			const res = await fetch(url, {
				method: 'HEAD',
			});

			const contentType = res.headers.get('Content-Type');

			if (contentType && SUPPORTED_IMG_TYPES.includes(contentType)) {
				return request<ImageModeration>('image', {
					url,
				});
			}
		},
		placeholderData,
		staleTime: 24 * 60 * 60 * 1000,
		cacheTime: Infinity,
	});
}
