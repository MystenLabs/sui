// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';

import { registerUnsafeBurnerWallet } from '../../core/wallet/registerUnsafeBurnerWallet.js';
import { useSuiClient } from '../useSuiClient.js';

export function useUnsafeBurnerWallet(enabled: boolean) {
	const suiClient = useSuiClient();

	useEffect(() => {
		if (!enabled) {
			return;
		}
		const unregister = registerUnsafeBurnerWallet(suiClient);
		return unregister;
	}, [enabled, suiClient]);
}
