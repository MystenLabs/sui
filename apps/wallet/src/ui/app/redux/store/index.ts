// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { configureStore } from '@reduxjs/toolkit';

import { thunkExtras } from './thunk-extras';
import rootReducer from '_redux/RootReducer';

const store = configureStore({
    reducer: rootReducer,
    middleware: (getDefaultMiddleware) =>
        getDefaultMiddleware({
            thunk: {
                extraArgument: thunkExtras,
            },
        }),
});

export default store;

export type AppDispatch = typeof store.dispatch;
