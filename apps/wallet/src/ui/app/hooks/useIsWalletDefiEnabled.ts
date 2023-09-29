// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import useAppSelector from '_app/hooks/useAppSelector';
import { API_ENV } from '_shared/api-env';
import { FEATURES } from '_shared/experimentation/features';
import { useFeature } from '@growthbook/growthbook-react';

export function useIsWalletDefiEnabled() {
	const isDefiWalletEnabled = useFeature<boolean>(FEATURES.WALLET_DEFI).value;
	const { apiEnv } = useAppSelector((state) => state.app);

	return apiEnv === API_ENV.mainnet && isDefiWalletEnabled;
}
