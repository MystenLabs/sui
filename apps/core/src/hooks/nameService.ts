// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeatureIsOn } from '@growthbook/growthbook-react';
import { useSuiClient } from '@mysten/dapp-kit';
import { useQuery } from '@tanstack/react-query';

const SUI_NS_FEATURE_FLAG = 'suins';

// This should align with whatever names we want to be able to resolve.

export function useSuiNSEnabled() {
	return useFeatureIsOn(SUI_NS_FEATURE_FLAG);
}

export function useResolveSuiNSAddress(name?: string | null, enabled?: boolean) {
	const client = useSuiClient();
	const enabledSuiNs = useSuiNSEnabled();

	return useQuery({
		queryKey: ['resolve-suins-address', name],
		queryFn: async () => {
			return await client.resolveNameServiceAddress({
				name: name!,
			});
		},
		enabled: !!name && enabled && enabledSuiNs,
		refetchOnWindowFocus: false,
		retry: false,
	});
}

export function useResolveSuiNSName(address?: string | null) {
	const client = useSuiClient();
	const enabled = useSuiNSEnabled();

	return useQuery({
		queryKey: ['resolve-suins-name', address],
		queryFn: async () => {
			// NOTE: We only fetch 1 here because it's the default name.
			const { data } = await client.resolveNameServiceNames({
				address: address!,
				limit: 1,
			});

			return data[0] || null;
		},
		enabled: !!address && enabled,
		refetchOnWindowFocus: false,
		retry: false,
	});
}
