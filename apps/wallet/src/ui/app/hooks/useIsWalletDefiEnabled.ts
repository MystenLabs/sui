// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import useAppSelector from '_app/hooks/useAppSelector';
import { FEATURES } from '_shared/experimentation/features';
import { useFeature } from '@growthbook/growthbook-react';

export function useIsWalletDefiEnabled() {
	const isDefiWalletEnabled = useFeature<boolean>(FEATURES.WALLET_DEFI).value;
	const { apiEnv, customRPC } = useAppSelector((state) => state.app);
	const activeNetwork = customRPC && apiEnv === 'customRPC' ? customRPC : apiEnv.toUpperCase();

	return activeNetwork === 'MAINNET' && isDefiWalletEnabled;
}
