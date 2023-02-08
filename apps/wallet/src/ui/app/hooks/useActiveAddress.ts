// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useActiveAccount } from './useActiveAccount';
import type { SuiAddress } from '@mysten/sui.js/src';

export function useActiveAddress(): SuiAddress | null {
    return useActiveAccount()?.address || null;
}
