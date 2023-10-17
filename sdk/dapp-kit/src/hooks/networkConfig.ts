// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClientOptions } from '@mysten/sui.js/client';

import { useSuiClientContext } from './useSuiClient.js';

export function createNetworkConfig<T extends Record<string, SuiClientOptions>>(
	networkConfigs: T,
): { networkConfigs: T; useNetworkConfig: () => T[keyof T] } {
	return {
		networkConfigs,
		useNetworkConfig: () => {
			const { config } = useSuiClientContext();

			if (!config) {
				throw new Error('No network config found');
			}

			return config as T[keyof T];
		},
	};
}
