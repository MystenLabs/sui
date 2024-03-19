// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { ZkSendWallet } from '@mysten/zksend';
import { registerZkSendWallet } from '@mysten/zksend';
import { useEffect, useLayoutEffect, useState } from 'react';

import { useAutoConnectWallet } from './useAutoConnectWallet.js';
import { useConnectWallet } from './useConnectWallet.js';

export interface ZkSendWalletConfig {
	name: string;
	origin?: string;
}

export function useZkSendWallet(config?: ZkSendWalletConfig) {
	const status = useAutoConnectWallet();
	const [address, setAddress] = useState<string | null>(null);
	const [wallet, setWallet] = useState<ZkSendWallet | null>(null);
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

		const { wallet, unregister, addressFromRedirect } = registerZkSendWallet(config.name, {
			origin: config.origin,
		});

		if (addressFromRedirect) {
			setWallet(wallet);
			setAddress(addressFromRedirect);
		}

		return unregister;
	}, [config?.name, config?.origin]);
}
