// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { StashedWallet } from '@mysten/zksend';
import { registerStashedWallet } from '@mysten/zksend';
import { useEffect, useLayoutEffect, useState } from 'react';

import { useAutoConnectWallet } from './useAutoConnectWallet.js';
import { useConnectWallet } from './useConnectWallet.js';

export interface StashedWalletConfig {
	name: string;
	network?: 'mainnet' | 'testnet';
	origin?: string;
}

export function useStashedWallet(config?: StashedWalletConfig) {
	const status = useAutoConnectWallet();
	const [address, setAddress] = useState<string | null>(null);
	const [wallet, setWallet] = useState<StashedWallet | null>(null);
	const { mutate: connect } = useConnectWallet();

	useEffect(() => {
		// This handles an edge case where the user has already connected a wallet, but is coming from
		// a zkSend redirect, and we want to force the zkSend wallet to connect. We need to wait for the
		// autoconnection to attempt to connect, then force the zkSend wallet to connect.
		if (!address || !wallet || status !== 'attempted') return;

		connect({ wallet, silent: true });
		// Reset the address since we only want to do this once:
		setAddress(null);
	}, [address, status, connect, wallet]);

	useLayoutEffect(() => {
		if (!config?.name) {
			return;
		}

		const { wallet, unregister, addressFromRedirect } = registerStashedWallet(config.name, {
			origin: config.origin,
			network: config.network,
		});

		if (addressFromRedirect) {
			setWallet(wallet);
			setAddress(addressFromRedirect);
		}

		return unregister;
	}, [config?.name, config?.origin, config?.network]);
}
