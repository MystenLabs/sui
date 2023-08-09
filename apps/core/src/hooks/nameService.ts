// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useQuery } from '@tanstack/react-query';
import { useRpcClient } from '../api/RpcClientContext';
import { useFeatureIsOn } from '@growthbook/growthbook-react';

const SUI_NS_FEATURE_FLAG = 'suins';

// This should align with whatever names we want to be able to resolve.
const SUI_NS_DOMAINS = ['.sui'];
export function isSuiNSName(name: string) {
	return SUI_NS_DOMAINS.some((domain) => name.endsWith(domain));
}

export function useSuiNSEnabled() {
	return useFeatureIsOn(SUI_NS_FEATURE_FLAG);
}

export function useResolveSuiNSAddress(name?: string | null) {
	const rpc = useRpcClient();
	const enabled = useSuiNSEnabled();

	return useQuery({
		queryKey: ['resolve-suins-address', name],
		queryFn: async () => {
			return await rpc.resolveNameServiceAddress({
				name: name!,
			});
		},
		enabled: !!name && enabled,
		refetchOnWindowFocus: false,
		retry: false,
	});
}

export function useResolveSuiNSName(address?: string | null) {
	const rpc = useRpcClient();
	const enabled = useSuiNSEnabled();

	return useQuery({
		queryKey: ['resolve-suins-name', address],
		queryFn: async () => {
			// NOTE: We only fetch 1 here because it's the default name.
			const { data } = await rpc.resolveNameServiceNames({
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
