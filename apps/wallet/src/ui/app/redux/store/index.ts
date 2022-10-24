// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { configureStore } from '@reduxjs/toolkit';

import { KeypairVaultMiddleware } from './middlewares/KeypairVaultMiddleware';
import { NetworkSwitchMiddleware } from './middlewares/NetworkSwitchMiddleware';
import { thunkExtras } from './thunk-extras';
import { FaucetRequestGasMiddleware } from '_app/shared/faucet/middlewares';
import rootReducer from '_redux/RootReducer';

const store = configureStore({
    reducer: rootReducer,
    middleware: (getDefaultMiddleware) =>
        getDefaultMiddleware({
            thunk: {
                extraArgument: thunkExtras,
            },
        }).concat(
            KeypairVaultMiddleware,
            NetworkSwitchMiddleware,
            FaucetRequestGasMiddleware
        ),
});

export default store;

export type AppDispatch = typeof store.dispatch;
