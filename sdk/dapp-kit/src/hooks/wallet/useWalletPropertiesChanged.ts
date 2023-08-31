// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import type {
	WalletWithSuiFeatures,
	StandardEventsChangeProperties,
} from '@mysten/wallet-standard';

/**
 * Internal hook for easily handling various changes in properties for a wallet.
 */
export function useWalletPropertiesChanged(
	wallet: WalletWithSuiFeatures | null,
	onChange: (updatedProperties: StandardEventsChangeProperties) => void,
) {
	useEffect(() => {
		const unsubscribeFromEvents = wallet?.features['standard:events'].on('change', onChange);
		return unsubscribeFromEvents;
	}, [wallet, onChange]);
}
