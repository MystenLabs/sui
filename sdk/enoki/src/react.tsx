// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { SuiClient } from '@mysten/sui/client';
import { useStore } from '@nanostores/react';
import type { ReactNode } from 'react';
import { createContext, useContext, useEffect, useMemo, useState } from 'react';

import type { EnokiFlowConfig } from './EnokiFlow.js';
import { EnokiFlow } from './EnokiFlow.js';
import { registerEnokiWallets } from './wallet/index.js';

const EnokiFlowContext = createContext<EnokiFlow | null>(null);

export interface EnokiFlowProviderProps extends EnokiFlowConfig {
	children: ReactNode;
}

export function EnokiFlowProvider({ children, ...config }: EnokiFlowProviderProps) {
	const [enokiFlow] = useState(() => new EnokiFlow(config));
	return <EnokiFlowContext.Provider value={enokiFlow}>{children}</EnokiFlowContext.Provider>;
}

export function useEnokiFlow() {
	const context = useContext(EnokiFlowContext);
	if (!context) {
		throw new Error('Missing `EnokiFlowContext` provider');
	}
	return context;
}

export function useZkLogin() {
	const flow = useEnokiFlow();
	return useStore(flow.$zkLoginState);
}

export function useZkLoginSession() {
	const flow = useEnokiFlow();
	return useStore(flow.$zkLoginSession).value;
}

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
	flow: EnokiFlow;
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
	const { client, network } = useSuiClientContext();

	const { wallets, unregister, flow } = useMemo(
		() => registerEnokiWallets({ ...config, client, network }),
		[client, config, network],
	);

	useEffect(() => {
		return () => {
			unregister();
		};
	}, [unregister]);

	return (
		<EnokiWalletContext.Provider value={{ wallets, flow, client }}>
			{children}
		</EnokiWalletContext.Provider>
	);
}

export function useEnokiWallets() {
	const context = useContext(EnokiWalletContext);
	if (!context) {
		throw new Error('Missing `EnokiWalletContext` provider');
	}
	const { flow, wallets, client } = context;
	return {
		wallets,
		flow,
		execute: ({ bytes, signature }: { bytes: string; signature: string }) =>
			flow.executeSignedTransaction({
				bytes,
				signature,
				client,
			}),
	};
}
