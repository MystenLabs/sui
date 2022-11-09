// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isAnyOf } from '@reduxjs/toolkit';

import {
    loadEntropyFromKeyring,
    setAddress,
    createVault,
    setKeyringStatus,
} from '_redux/slices/account';
import { thunkExtras } from '_store/thunk-extras';

import type { Middleware } from '@reduxjs/toolkit';

const keypairVault = thunkExtras.keypairVault;
const matchUpdateKeypairVault = isAnyOf(
    loadEntropyFromKeyring.fulfilled,
    createVault.fulfilled,
    setKeyringStatus
);

export const KeypairVaultMiddleware: Middleware =
    ({ dispatch }) =>
    (next) =>
    (action) => {
        if (matchUpdateKeypairVault(action)) {
            let entropy;
            if (typeof action.payload === 'string') {
                entropy = action.payload;
            } else {
                entropy = action.payload?.entropy;
            }
            if (entropy) {
                keypairVault.entropy = entropy;
                dispatch(setAddress(keypairVault.getAccount()));
            } else {
                keypairVault.clear();
            }
            entropy = null;
        }
        return next(action);
    };
