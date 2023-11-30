// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiFeatures } from '@mysten/wallet-standard';
import { useQuery } from '@tanstack/react-query';
import type { UseQueryOptions, UseQueryResult } from '@tanstack/react-query';

import { useCurrentWallet } from './useCurrentWallet.js';

export interface DappKitWalletStandardFeatureMethods extends SuiFeatures {
	'foo:bar': {
		version: '1.0.0';
		bar: (name: string) => Promise<string>;
	};
}

type FunctionKeys<T> = {
	[K in keyof T]: T[K] extends (...args: any[]) => any ? K : never;
}[keyof T];

export function useFeatureQuery<
	Feature extends keyof DappKitWalletStandardFeatureMethods,
	Method extends FunctionKeys<
		DappKitWalletStandardFeatureMethods[Feature]
	> = (Feature extends `${string}:${infer M}` ? M : never) &
		FunctionKeys<DappKitWalletStandardFeatureMethods[Feature]>,
>(
	featureName: Feature,
	args?: Parameters<DappKitWalletStandardFeatureMethods[Feature][Method]>,
	options?: Omit<
		UseQueryOptions<Awaited<ReturnType<DappKitWalletStandardFeatureMethods[Feature][Method]>>>,
		'queryFn' | 'queryKey'
	>,
	methodName: Method = featureName.split(':')[1] as Method,
): UseQueryResult<Awaited<ReturnType<DappKitWalletStandardFeatureMethods[Feature][Method]>>> {
	const { currentWallet } = useCurrentWallet();

	return useQuery({
		...options,
		// eslint-disable-next-line @tanstack/query/exhaustive-deps
		queryKey: [
			'@mysten/dapp-kit',
			'wallet-feature',
			{ wallet: currentWallet?.name, featureName, methodName, args },
		],
		queryFn: async () => {
			if (!currentWallet) return null;

			const walletFeature = (currentWallet.features[featureName] as any)?.[methodName];

			if (!walletFeature) return null;

			return await walletFeature(...(args ?? []));
		},
		enabled: !!currentWallet,
	});
}

// eslint-disable-next-line react-hooks/rules-of-hooks
const { data } = useFeatureQuery('foo:bar', ['foo']);
void data;
