// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetObject } from '@mysten/core';
import { useMemo } from 'react';

export type NFTMetadata = {
	name: string | null;
	description: string | null;
	url: string;
};

export function useGetNFTMeta(objectID: string) {
	const resp = useGetObject(objectID);
	const nftMeta = useMemo(() => {
		if (!resp.data) return null;
		const display = resp.data.data?.display?.data;
		if (!display) {
			return null;
		}
		const { name, description, creator, image_url, link, project_url } = display;
		return {
			name: name || null,
			description: description || null,
			imageUrl: image_url || null,
			link: link || null,
			projectUrl: project_url || null,
			creator: creator || null,
		};
	}, [resp]);
	return {
		...resp,
		data: nftMeta,
	};
}
