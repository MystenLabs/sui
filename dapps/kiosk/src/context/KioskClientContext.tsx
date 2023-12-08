// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient } from '@mysten/dapp-kit';
import { KioskClient, Network } from '@mysten/kiosk';
import { createContext, ReactNode, useContext, useMemo } from 'react';

export const KioskClientContext = createContext<KioskClient | undefined>(undefined);

export function KisokClientProvider({ children }: { children: ReactNode }) {
	const suiClient = useSuiClient();
	const kioskClient = useMemo(
		() =>
			new KioskClient({
				client: suiClient,
				network: Network.TESTNET,
			}),
		[suiClient],
	);

	return <KioskClientContext.Provider value={kioskClient}>{children}</KioskClientContext.Provider>;
}

export function useKioskClient() {
	const kioskClient = useContext(KioskClientContext);
	if (!kioskClient) {
		throw new Error('kioskClient not setup properly.');
	}
	return kioskClient;
}
