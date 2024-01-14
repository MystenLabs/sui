// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { registerZkSendWallet } from '@mysten/zksend';
import { useEffect } from 'react';

export interface ZkSendWalletConfig {
	dappName: string;
	origin?: string;
}

export function useZkSendWallet(config?: ZkSendWalletConfig) {
	useEffect(() => {
		if (!config?.dappName) {
			return;
		}
		const unregister = registerZkSendWallet(config.dappName, { origin: config.origin });
		return unregister;
	}, [config?.dappName, config?.origin]);
}
