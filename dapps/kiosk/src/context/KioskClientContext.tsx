// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClient, useSuiClientContext } from '@mysten/dapp-kit';
import { KioskClient, Network } from '@mysten/kiosk';
import { createContext, ReactNode, useContext, useMemo } from 'react';

export const KioskClientContext = createContext<KioskClient | undefined>(undefined);

export function KioskClientProvider({ children }: { children: ReactNode }) {
	const suiClient = useSuiClient();
	const { network } = useSuiClientContext();
	const kioskClient = useMemo(
		() =>
			new KioskClient({
				client: suiClient,
				network: network as Network,
			}),
		[suiClient, network],
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
