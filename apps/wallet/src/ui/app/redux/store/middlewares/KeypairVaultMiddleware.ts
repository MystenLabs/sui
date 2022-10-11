// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isAnyOf } from '@reduxjs/toolkit';

import {
    loadMnemonicFromKeyring,
    setAddress,
    createMnemonic,
    setKeyringStatus,
} from '_redux/slices/account';
import { thunkExtras } from '_store/thunk-extras';

import type { Middleware } from '@reduxjs/toolkit';

const keypairVault = thunkExtras.keypairVault;
const matchUpdateMnemonic = isAnyOf(
    loadMnemonicFromKeyring.fulfilled,
    createMnemonic.fulfilled,
    setKeyringStatus
);

export const KeypairVaultMiddleware: Middleware =
    ({ dispatch }) =>
    (next) =>
    (action) => {
        if (matchUpdateMnemonic(action)) {
            let mnemonic;
            if (typeof action.payload === 'string') {
                mnemonic = action.payload;
            } else {
                mnemonic = action.payload?.mnemonic;
            }
            if (mnemonic) {
                keypairVault.mnemonic = mnemonic;
                dispatch(setAddress(keypairVault.getAccount()));
            }
            mnemonic = null;
        }
        return next(action);
    };
