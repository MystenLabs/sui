// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWalletStore } from './useWalletStore.js';

/**
 * WIP will fill this out before landing lol (pinky promise)
 */
export function useAutoConnectStatus() {
	return useWalletStore((state) => state.autoConnectionStatus);
}
