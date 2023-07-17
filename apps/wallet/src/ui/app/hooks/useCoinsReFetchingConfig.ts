// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeatureValue } from '@growthbook/growthbook-react';

import { FEATURES } from '_src/shared/experimentation/features';

export function useCoinsReFetchingConfig() {
	const refetchInterval = useFeatureValue(FEATURES.WALLET_BALANCE_REFETCH_INTERVAL, 20_000);
	return {
		refetchInterval,
		staleTime: 5_000,
	};
}
