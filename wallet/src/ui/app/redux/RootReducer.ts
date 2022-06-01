// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { combineReducers } from '@reduxjs/toolkit';

import account from './slices/account';
import app from './slices/app';
import suiObjects from './slices/sui-objects';
import transactions from './slices/transactions';

const rootReducer = combineReducers({ account, app, suiObjects, transactions });

export type RootState = ReturnType<typeof rootReducer>;

export default rootReducer;
