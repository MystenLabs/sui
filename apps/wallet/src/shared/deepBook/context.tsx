// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useDeepBookConfigs } from '_app/hooks/deepbook/useDeepBookConfigs';
import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { DEFAULT_WALLET_FEE_ADDRESS, type Coins } from '_pages/swap/constants';
import { FEATURES } from '_shared/experimentation/features';
import { useFeatureValue } from '@growthbook/growthbook-react';
import { useGetOwnedObjects } from '@mysten/core';
import { useSuiClient } from '@mysten/dapp-kit';
import { DeepBookClient } from '@mysten/deepbook';
import { createContext, useContext, useMemo, type ReactNode } from 'react';

type DeepBookContextProps = {
	client: DeepBookClient;
	accountCapId: string;
	configs: {
		pools: Record<string, string[]>;
		coinsMap: Record<Coins, string>;
	};
	walletFeeAddress: string;
};

const DeepBookContext = createContext<DeepBookContextProps | null>(null);

interface DeepBookContextProviderProps {
	children: ReactNode;
}

export function useDeepBookContext() {
	const context = useContext(DeepBookContext);
	if (!context) {
		throw new Error('useDeepBookContext must be used within a DeepBookContextProvider');
	}
	return context;
}

export function DeepBookContextProvider({ children }: DeepBookContextProviderProps) {
	const suiClient = useSuiClient();
	const activeAccount = useActiveAccount();
	const activeAccountAddress = activeAccount?.address;

	const configs = useDeepBookConfigs();
	const walletFeeAddress = useFeatureValue(FEATURES.WALLET_FEE_ADDRESS, DEFAULT_WALLET_FEE_ADDRESS);

	const { data } = useGetOwnedObjects(
		activeAccountAddress,
		{ StructType: '0xdee9::custodian_v2::AccountCap' },
		1,
	);

	const objectContent = data?.pages?.[0]?.data?.[0]?.data?.content;
	const objectFields = objectContent?.dataType === 'moveObject' ? objectContent?.fields : null;

	const accountCapId = (objectFields as Record<string, string | number | object>)?.owner as string;

	const deepBookClient = useMemo(() => {
		return new DeepBookClient(suiClient, accountCapId);
	}, [accountCapId, suiClient]);

	const contextValue = useMemo(() => {
		return {
			client: deepBookClient,
			accountCapId,
			configs,
			walletFeeAddress,
		};
	}, [accountCapId, configs, deepBookClient, walletFeeAddress]);

	return <DeepBookContext.Provider value={contextValue}>{children}</DeepBookContext.Provider>;
}
