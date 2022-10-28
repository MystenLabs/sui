// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isAnyOf } from '@reduxjs/toolkit';

import { requestGas } from './actions';
import { clearLastRequest } from './slice';

import type { Middleware } from '@reduxjs/toolkit';

const FAUCET_REQUEST_GAS_CLEAR_TIMEOUT = 1000 * 3;
const isFaucetRequestGasStarted = isAnyOf(requestGas.pending);
const isFaucetRequestGasFinally = isAnyOf(
    requestGas.rejected,
    requestGas.fulfilled
);

let clearRequestGatTimeout: number | null = null;

export const FaucetRequestGasMiddleware: Middleware =
    ({ dispatch }) =>
    (next) =>
    (action) => {
        if (isFaucetRequestGasStarted(action) && clearRequestGatTimeout) {
            clearTimeout(clearRequestGatTimeout);
            clearRequestGatTimeout = null;
        } else if (isFaucetRequestGasFinally(action)) {
            if (clearRequestGatTimeout) {
                clearTimeout(clearRequestGatTimeout);
            }
            clearRequestGatTimeout = window.setTimeout(
                () => dispatch(clearLastRequest()),
                FAUCET_REQUEST_GAS_CLEAR_TIMEOUT
            );
        }
        return next(action);
    };
