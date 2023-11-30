// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeatureQuery } from '@mysten/dapp-kit';

declare module '@mysten/dapp-kit' {
	interface SuiWalletStandardFeatureMethods {
		'enoki:getStuff': {
			version: '1.0.0';
			getStuff: () => Promise<{
				stuff: string;
			}>;
		};
	}
}

// eslint-disable-next-line react-hooks/rules-of-hooks
const { data } = useFeatureQuery('enoki:getStuff');
void data;
