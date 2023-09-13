// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useDAppKitStore } from '../useDAppKitStore.js';

export function useCurrentAccount() {
	return useDAppKitStore((state) => state.currentAccount);
}
