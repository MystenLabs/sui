// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useActiveAccount } from './useActiveAccount';
import { thunkExtras } from '_redux/store/thunk-extras';

import type { SuiAddress } from '@mysten/sui.js';

export function useSigner() {
    const signerAccount = useActiveAccount();
    const { api, background, initAppSui } = thunkExtras;
    if (!signerAccount) {
        return null;
    }
    return api.getSignerInstance(signerAccount, background, initAppSui);
}
