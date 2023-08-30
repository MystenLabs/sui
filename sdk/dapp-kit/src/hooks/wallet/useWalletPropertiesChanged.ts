// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import type {
	WalletWithSuiFeatures,
	StandardEventsChangeProperties,
} from '@mysten/wallet-standard';

export function useWalletPropertiesChanged(
	currentWallet: WalletWithSuiFeatures | null,
	onChange: (updatedProperties: StandardEventsChangeProperties) => void,
) {
	useEffect(() => {
		const unsubscribeFromEvents = currentWallet?.features['standard:events'].on('change', onChange);
		return unsubscribeFromEvents;
	}, [currentWallet, onChange]);
}
