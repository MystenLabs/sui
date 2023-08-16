// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useActiveAccount } from './useActiveAccount';

export function useActiveAddress() {
	return useActiveAccount()?.address || null;
}
