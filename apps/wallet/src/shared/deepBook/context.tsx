// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { useActiveAccount } from '_app/hooks/useActiveAccount';
import { useCreateAccount, useMarketAccountCap } from '_hooks';
import { useSuiClient } from '@mysten/dapp-kit';
import { DeepBookClient } from '@mysten/deepbook';
import { createContext, useContext, useEffect, useMemo, type ReactNode } from 'react';

type DeepBookContextProps = {
	client: DeepBookClient;
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

export function useDeepBookClient() {
	return useDeepBookContext().client;
}

export function DeepBookContextProvider({ children }: DeepBookContextProviderProps) {
	const suiClient = useSuiClient();
	const activeAccount = useActiveAccount();

	const { data, isLoading, refetch } = useMarketAccountCap(activeAccount?.address);

	const accountCapId = data?.owner as string;

	const deepBookClient = useMemo(() => {
		return new DeepBookClient(suiClient, accountCapId);
	}, [accountCapId, suiClient]);

	const { mutate } = useCreateAccount({
		onSuccess: refetch,
		deepBookClient,
	});

	useEffect(() => {
		if (!accountCapId && !isLoading) {
			mutate();
		}
	}, [accountCapId, isLoading, mutate]);

	return (
		<DeepBookContext.Provider
			value={{
				client: deepBookClient,
			}}
		>
			{children}
		</DeepBookContext.Provider>
	);
}
