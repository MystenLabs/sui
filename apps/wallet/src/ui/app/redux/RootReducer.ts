// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { combineReducers } from '@reduxjs/toolkit';

import account from './slices/account';
import app from './slices/app';
import permissions from './slices/permissions';
import transactionRequests from './slices/transaction-requests';

const rootReducer = combineReducers({
	account,
	app,
	permissions,
	transactionRequests,
});

export type RootState = ReturnType<typeof rootReducer>;

export default rootReducer;
