// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useSuiClientContext } from '@mysten/dapp-kit';
import { KioskClient, Network } from '@mysten/kiosk';
import { createContext, useMemo, type ReactNode } from 'react';

export const KioskClientContext = createContext<KioskClient | null>(null);

const suiToKioskNetwork: Record<string, Network> = {
	mainnet: Network.MAINNET,
	testnet: Network.TESTNET,
};

export type KioskClientProviderProps = {
	children: ReactNode;
};

export function KioskClientProvider({ children }: KioskClientProviderProps) {
	const { client, network } = useSuiClientContext();
	const kioskNetwork = suiToKioskNetwork[network.toLowerCase()] || Network.CUSTOM;
	const kioskClient = useMemo(
		() => new KioskClient({ client, network: kioskNetwork }),
		[client, kioskNetwork],
	);
	return <KioskClientContext.Provider value={kioskClient}>{children}</KioskClientContext.Provider>;
}
