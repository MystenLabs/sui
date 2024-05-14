// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useFeatureIsOn } from '@growthbook/growthbook-react';
import { normalizeSuiNSName } from '@mysten/sui.js/utils';

import { useResolveSuiNSName } from '../../../../../core';

export function useAppResolveSuinsName(address?: string) {
	const enableNewSuinsFormat = useFeatureIsOn('wallet-enable-new-suins-name-format');
	const { data } = useResolveSuiNSName(address);
	return data ? normalizeSuiNSName(data, enableNewSuinsFormat ? 'at' : 'dot') : undefined;
}
