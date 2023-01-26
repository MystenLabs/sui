// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { thunkExtras } from '_redux/store/thunk-extras';

export function useSigner() {
    const { api, keypairVault, background } = thunkExtras;
    return api.getSignerInstance(
        keypairVault.getKeypair().getPublicKey().toSuiAddress(),
        background
    );
}
