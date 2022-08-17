// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer, type SignaturePubkeyPair } from '@mysten/sui.js';
import {
    createAsyncThunk,
    createEntityAdapter,
    createSlice,
    type EntityState,
} from '@reduxjs/toolkit';

import type { PayloadAction } from '@reduxjs/toolkit';
import type { SignMessageRequest } from '_payloads/messages/SignMessageRequest';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

const signMessageRequestsAdapter = createEntityAdapter<SignMessageRequest>({
    sortComparer: (a, b) => {
        const aDate: Date = new Date(a.createdDate);
        const bDate: Date = new Date(b.createdDate);
        return aDate.getTime() - bDate.getTime();
    },
});

export const respondToSignMessageRequest = createAsyncThunk<
    {
        id: string;
        approved: boolean;
        signature: SignaturePubkeyPair | null;
    },
    { id: string; approved: boolean },
    AppThunkConfig
>(
    'respond-to-sign-message-request',
    async (
        { id, approved },
        { extra: { background, api, keypairVault }, getState }
    ) => {
        const state = getState();
        const signMessageRequest = signMessageRequestsSelectors.selectById(
            state,
            id
        );
        if (!signMessageRequest) {
            throw new Error(`SignMessageRequest ${id} not found`);
        }
        let signMessageResult: SignaturePubkeyPair | undefined = undefined;
        let signMessageResultError: string | undefined;
        if (approved) {
            const signer = api.getSignerInstance(keypairVault.getKeyPair());
            try {
                if (signMessageRequest.message) {
                    signMessageResult = await signer.signData(
                        new Base64DataBuffer(signMessageRequest.message)
                    );
                } else {
                    throw new Error('Message must be defined.');
                }
            } catch (e) {
                signMessageResultError = (e as Error).message;
            }
        }
        background.sendSignMessageRequestResponse(
            id,
            approved,
            signMessageResult,
            signMessageResultError
        );
        return { id, approved: approved, signature: null };
    }
);

type State = EntityState<SignMessageRequest> & {
    initialized: boolean;
};

const slice = createSlice({
    name: 'sign-message-requests',
    initialState: signMessageRequestsAdapter.getInitialState({
        initialized: false,
    }),
    reducers: {
        setSignMessageRequests: (
            state,
            { payload }: PayloadAction<SignMessageRequest[]>
        ) => {
            signMessageRequestsAdapter.setAll(state as State, payload);
            state.initialized = true;
        },
    },
    extraReducers: (build) => {
        build.addCase(
            respondToSignMessageRequest.fulfilled,
            (state, { payload }) => {
                const { id, signature, approved: allowed } = payload;
                signMessageRequestsAdapter.updateOne(state as State, {
                    id,
                    changes: {
                        approved: allowed,
                        signature: signature || undefined,
                    },
                });
            }
        );
    },
});

export default slice.reducer;

export const { setSignMessageRequests } = slice.actions;

export const signMessageRequestsSelectors =
    signMessageRequestsAdapter.getSelectors(
        (state: RootState) => state.signMessageRequests
    );
