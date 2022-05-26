// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { combineReducers } from '@reduxjs/toolkit';

import account from './slices/account';
import app from './slices/app';
import suiObjects from './slices/sui-objects';

const rootReducer = combineReducers({ account, app, suiObjects });

export type RootState = ReturnType<typeof rootReducer>;

export default rootReducer;
