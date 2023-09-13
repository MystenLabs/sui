// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useDAppKitStore } from '../useDAppKitStore.js';

export function useCurrentWallet() {
	const a = useDAppKitStore((state) => state);
	console.log('WHOLEST ATE', a);
	return useDAppKitStore((state) => state.currentWallet);
}
