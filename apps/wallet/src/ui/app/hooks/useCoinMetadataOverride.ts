// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { FEATURES } from '_shared/experimentation/features';
import { useFeature } from '@growthbook/growthbook-react';

interface CoinMetadataOverride {
	[coinType: string]: {
		name?: string;
		iconUrl?: string;
	};
}

export function useCoinMetadataOverrides() {
	const coinMetadataOverrides = useFeature<CoinMetadataOverride>(
		FEATURES.TOKEN_METADATA_OVERRIDES,
	).value;

	return coinMetadataOverrides || {};
}
