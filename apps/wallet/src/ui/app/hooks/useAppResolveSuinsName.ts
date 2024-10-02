// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useFeatureIsOn } from '@growthbook/growthbook-react';
import { normalizeSuiNSName } from '@mysten/sui/utils';

import { useResolveSuiNSName as useResolveSuiNSNameCore } from '../../../../../core';

export function useResolveSuiNSName(address?: string) {
	const enableNewSuinsFormat = useFeatureIsOn('wallet-enable-new-suins-name-format');
	const { data } = useResolveSuiNSNameCore(address);
	return data ? normalizeSuiNSName(data, enableNewSuinsFormat ? 'at' : 'dot') : undefined;
}
