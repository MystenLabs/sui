// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isAnyOf } from '@reduxjs/toolkit';

import {
    loadAccountFromStorage,
    setMnemonic,
    setAddress,
} from '_redux/slices/account';
import { thunkExtras } from '_store/thunk-extras';

import type { Middleware } from '@reduxjs/toolkit';

const keypairVault = thunkExtras.keypairVault;
const matchUpdateMnemonic = isAnyOf(
    loadAccountFromStorage.fulfilled,
    setMnemonic
);

export const KeypairVaultMiddleware: Middleware =
    ({ dispatch }) =>
    (next) =>
    (action) => {
        if (matchUpdateMnemonic(action)) {
            if (action.payload) {
                keypairVault.mnemonic = action.payload;
                dispatch(setAddress(keypairVault.getAccount()));
            }
        }
        return next(action);
    };
