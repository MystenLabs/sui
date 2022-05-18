// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { configureStore } from '@reduxjs/toolkit';

import { KeypairVaultMiddleware } from './middlewares/KeypairVaultMiddleware';
import { thunkExtras } from './thunk-extras';
import rootReducer from '_redux/RootReducer';

const store = configureStore({
    reducer: rootReducer,
    middleware: (getDefaultMiddleware) =>
        getDefaultMiddleware({
            thunk: {
                extraArgument: thunkExtras,
            },
        }).concat(KeypairVaultMiddleware),
});

export default store;

export type AppDispatch = typeof store.dispatch;
