// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui.js/client';
import { createContext, useContext } from 'react';

export const SuiClientContext = createContext<SuiClient | undefined>(undefined);

export function useSuiClient() {
	const suiClient = useContext(SuiClientContext);

	if (!suiClient) {
		throw new Error(
			'Could not find SuiClientContext. Ensure that you have set up the SuiClientProvider',
		);
	}

	return suiClient;
}
