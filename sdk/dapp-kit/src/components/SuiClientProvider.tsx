// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClient, getFullnodeUrl } from '@mysten/sui.js/client';
import { SuiClientContext } from '../hooks/useSuiClient.js';
import { useMemo } from 'react';

export interface SuiClientProviderProps {
	children: React.ReactNode;
	client?: SuiClient;
	url?: string;
}

export const SuiClientProvider = (props: SuiClientProviderProps) => {
	const client = useMemo(
		() =>
			props.client ??
			new SuiClient({
				url: props.url ?? getFullnodeUrl('devnet'),
			}),
		[props.client, props.url],
	);

	return <SuiClientContext.Provider value={client}>{props.children}</SuiClientContext.Provider>;
};
