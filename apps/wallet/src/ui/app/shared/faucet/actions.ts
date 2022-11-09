// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createAsyncThunk } from '@reduxjs/toolkit';

import { ENV_TO_API } from '_app/ApiProvider';

import type { ObjectId, TransactionDigest } from '@mysten/sui.js';
import type { AppThunkConfig } from '_redux/store/thunk-extras';

type RequestGasResponse = {
    transferred_gas_objects: [
        {
            amount: number;
            id: ObjectId;
            transfer_tx_digest: TransactionDigest;
        }
    ];
    error: unknown | null;
};

export const requestGas = createAsyncThunk<
    { total: number; status: number; statusTxt: string },
    void,
    AppThunkConfig & {
        rejectValue: { status: number; statusTxt: string; retryAfter?: number };
    }
>('faucet-request-gas', async (_, { getState, rejectWithValue }) => {
    const {
        app: { apiEnv },
        account: { address },
    } = getState();
    const faucetUrl = new URL('/gas', ENV_TO_API[apiEnv]?.faucet);
    if (!address) {
        throw rejectWithValue({
            status: -1,
            statusTxt: 'Failed, wallet address not found.',
        });
    }
    let res;
    try {
        res = await fetch(faucetUrl, {
            method: 'post',
            headers: {
                'Content-Type': 'application/json',
            },
            body: JSON.stringify({
                FixedAmountRequest: { recipient: address },
            }),
        });
    } catch (e) {
        throw rejectWithValue({
            status: -2,
            statusTxt: (e as Error).message,
        });
    }
    if (!res.ok) {
        throw rejectWithValue({
            status: res.status,
            statusTxt: res.statusText,
            retryAfter: res.headers.has('retry-after')
                ? Number(res.headers.get('retry-after'))
                : undefined,
        });
    }
    try {
        const result: RequestGasResponse = await res.json();
        const total = result.transferred_gas_objects.reduce(
            (acc, anObj) => acc + anObj.amount,
            0
        );
        return {
            total,
            status: res.status,
            statusTxt: res.statusText,
        };
    } catch (e) {
        throw rejectWithValue({
            status: -3,
            statusTxt: (e as Error).message,
        });
    }
});
