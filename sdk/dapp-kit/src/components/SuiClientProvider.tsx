// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SuiClient, getFullnodeUrl } from '@mysten/sui.js/client';
import { SuiClientContext } from '../hooks/useSuiClient.js';
import { useMemo } from 'react';

export interface SuiClientProviderProps {
	children: React.ReactNode;
	client?: SuiClient;
	url?: string;
	queryKeyPrefix: string;
}

export const SuiClientProvider = (props: SuiClientProviderProps) => {
	const ctx = useMemo(() => {
		const client =
			props.client ??
			new SuiClient({
				url: props.url ?? getFullnodeUrl('devnet'),
			});

		return {
			client,
			queryKey: (key: unknown[]) => [props.queryKeyPrefix, ...key],
		};
	}, [props.client, props.url, props.queryKeyPrefix]);

	return <SuiClientContext.Provider value={ctx}>{props.children}</SuiClientContext.Provider>;
};
