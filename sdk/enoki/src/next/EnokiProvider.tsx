// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Wallet } from '@mysten/wallet-standard';
import { getWallets } from '@mysten/wallet-standard';
import { createContext, useContext, useLayoutEffect, useMemo, useState } from 'react';
import type { ReactNode } from 'react';

import { EnokiFlow } from '../EnokiFlow.js';
import type { EnokiFlowConfig } from '../EnokiFlow.js';
import { EnokiWallet } from './EnokiWallet.js';

export type AuthProvider = 'google' | 'twitch' | 'facebook';
export interface ProviderConfig {
	provider: 'google' | 'twitch' | 'facebook';
	clientId: string;
}

const EnokiContext = createContext<{
	wallets: Record<string, Wallet>;
} | null>(null);

export function useEnoki() {
	const context = useContext(EnokiContext);

	if (!context) {
		throw new Error('Missing `EnokiContext` provider');
	}

	return context;
}

export function useEnokiWallet(name: string) {
	const { wallets } = useEnoki();

	if (!wallets[name]) {
		throw new Error(`Missing wallet config for "${name}"`);
	}

	return wallets[name];
}

export interface EnokiProviderProps extends EnokiFlowConfig {
	children: ReactNode;
	providers: Record<string, ProviderConfig>;
}

export function EnokiProvider({ children, providers, ...config }: EnokiProviderProps) {
	const [enokiFlow] = useState(() => new EnokiFlow(config));
	const wallets = useMemo(
		() =>
			Object.fromEntries(
				Object.entries(providers).map(([key, { provider, clientId }]) => [
					key,
					new EnokiWallet(enokiFlow, provider, clientId),
				]),
			),
		[providers, enokiFlow],
	);

	useLayoutEffect(() => {
		const api = getWallets();
		const unregisterCallbacks = Object.values(wallets).map((wallet) => api.register(wallet));

		return () => {
			unregisterCallbacks.forEach((unregister) => unregister());
		};
	}, [wallets]);

	return <EnokiContext.Provider value={{ wallets }}>{children}</EnokiContext.Provider>;
}
