// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiFeatures } from '@mysten/wallet-standard';
import type { UseQueryResult } from '@tanstack/react-query';

export interface SuiWalletStandardFeatureMethods extends SuiFeatures {}

type FunctionKeys<T> = {
	[K in keyof T]: T[K] extends (...args: any[]) => any ? K : never;
}[keyof T];

export function useFeatureQuery<
	Feature extends keyof SuiWalletStandardFeatureMethods,
	Method extends FunctionKeys<
		SuiWalletStandardFeatureMethods[Feature]
	> = (Feature extends `${string}:${infer M}` ? M : never) &
		FunctionKeys<SuiWalletStandardFeatureMethods[Feature]>,
>(
	feature: Feature,
	method: Method = feature.split(':')[1] as Method,
): UseQueryResult<Awaited<ReturnType<SuiWalletStandardFeatureMethods[Feature][Method]>>> {
	void method;
	throw new Error('Not implemented');
}

// eslint-disable-next-line react-hooks/rules-of-hooks
const { data } = useFeatureQuery('sui:signPersonalMessage');
void data;
