// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui/client';
import { useStore } from '@nanostores/react';
import type { ReactNode } from 'react';
import { createContext, useContext, useEffect, useMemo, useState } from 'react';

import type { AuthProvider, EnokiFlowConfig } from './EnokiFlow.js';
import { EnokiFlow } from './EnokiFlow.js';
import type { EnokiWallet } from './wallet/index.js';
import { registerEnokiWallets } from './wallet/index.js';

const EnokiFlowContext = createContext<EnokiFlow | null>(null);

export interface EnokiFlowProviderProps extends EnokiFlowConfig {
	children: ReactNode;
}

/** @deprecated use EnokiWalletProvider instead  */
export function EnokiFlowProvider({ children, ...config }: EnokiFlowProviderProps) {
	const [enokiFlow] = useState(() => new EnokiFlow(config));
	return <EnokiFlowContext.Provider value={enokiFlow}>{children}</EnokiFlowContext.Provider>;
}

/** @deprecated use EnokiFlowProvider and dapp-kit wallet hooks instead */
export function useEnokiFlow() {
	const context = useContext(EnokiFlowContext);
	if (!context) {
		throw new Error('Missing `EnokiFlowContext` provider');
	}
	return context;
}

/** @deprecated use EnokiFlowProvider and dapp-kit wallet hooks instead */
export function useZkLogin() {
	const flow = useEnokiFlow();
	return useStore(flow.$zkLoginState);
}

/** @deprecated use EnokiFlowProvider and dapp-kit wallet hooks instead */
export function useZkLoginSession() {
	const flow = useEnokiFlow();
	return useStore(flow.$zkLoginSession).value;
}

/** @deprecated use EnokiFlowProvider and dapp-kit wallet hooks instead */
export function useAuthCallback() {
	const flow = useEnokiFlow();
	const [state, setState] = useState<string | null>(null);
	const [handled, setHandled] = useState(false);
	const [hash, setHash] = useState<string | null>(null);

	useEffect(() => {
		const listener = () => setHash(window.location.hash.slice(1).trim());
		listener();

		window.addEventListener('hashchange', listener);
		return () => window.removeEventListener('hashchange', listener);
	}, []);

	useEffect(() => {
		if (!hash) return;

		(async () => {
			try {
				setState(await flow.handleAuthCallback(hash));

				window.location.hash = '';
			} finally {
				setHandled(true);
			}
		})();
	}, [hash, flow]);

	return { handled, state };
}

export const EnokiWalletContext = createContext<{
	wallets: ReturnType<typeof registerEnokiWallets>['wallets'];
	client: SuiClient;
} | null>(null);

export function EnokiWalletProvider({
	children,
	config,
	useSuiClientContext,
}: {
	config: Omit<Parameters<typeof registerEnokiWallets>[0], 'client'>;
	useSuiClientContext: () => { client: SuiClient; network: string };
	children: React.ReactNode;
}) {
	const [wallets, setWallets] = useState<Partial<Record<AuthProvider, EnokiWallet>>>({});
	const { client, network } = useSuiClientContext();

	useEffect(() => {
		const { wallets, unregister } = registerEnokiWallets({ ...config, client, network });

		setWallets(wallets);
		return unregister;
	}, [client, config, network]);

	const values = useMemo(() => ({ wallets, client }), [wallets, client]);

	return <EnokiWalletContext.Provider value={values}>{children}</EnokiWalletContext.Provider>;
}

export function useEnokiWallets() {
	const context = useContext(EnokiWalletContext);
	if (!context) {
		throw new Error('Missing `EnokiWalletContext` provider');
	}
	return context;
}
