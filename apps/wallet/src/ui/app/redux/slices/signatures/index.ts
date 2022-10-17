// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
  createAsyncThunk,
  createEntityAdapter,
  createSlice,
  type PayloadAction
} from '@reduxjs/toolkit';

import type { SuiSignMessageOutput } from '@mysten/wallet-standard';
import type { SignatureRequest } from "_payloads/signatures";
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

const sigRequestsAdapter = createEntityAdapter<SignatureRequest>({
  sortComparer: (a, b) => {
    const aDate = new Date(a.createdDate);
    const bDate = new Date(b.createdDate);
    return aDate.getTime() - bDate.getTime();
  },
});

export const respondToSignatureRequest = createAsyncThunk<
  {
    sigRequestId: string;
    signed: boolean;
    signature: SuiSignMessageOutput | null;
  },
  { sigRequestId: string; signed: boolean; },
  AppThunkConfig
>(
  'respond-to-signature-request',
  async (
    { sigRequestId, signed },
    { extra: { background, api, keypairVault }, getState }
  ) => {
    const state = getState();
    const sigRequest = sigRequestsSelectors.selectById(state, sigRequestId);
    if (!sigRequest)
      throw new Error(`SignatureRequest ${sigRequestId} not found`);
    let sigResult: SuiSignMessageOutput | undefined = undefined;
    let sigResultError: string | undefined;
    if (signed) {
      const signer = api.getSignerInstance(keypairVault.getKeyPair());
      try {
        const data = [];
        for (let i = 0; i < Object.keys(sigRequest.message).length; i++)
          data.push(sigRequest.message[i]);
        sigResult = await signer.signMessage(Uint8Array.from(data));
      } catch (err) {
        sigResultError = (err as Error).message;
      }
    }
    background.sendSignatureRequestResponse(
      sigRequestId,
      signed,
      sigResult,
      sigResultError
    );
    return { sigRequestId, signed: signed, signature: null };
  }
);

const slice = createSlice({
  name: 'signature-requests',
  initialState: sigRequestsAdapter.getInitialState({ initialized: false }),
  reducers: {
    setSignatureRequests: (
      state,
      { payload }: PayloadAction<SignatureRequest[]>
    ) => {
      // eslint-disable-next-line @typescript-eslint/ban-ts-comment
      // @ts-ignore
      sigRequestsAdapter.setAll(state, payload);
      state.initialized = true;
    }
  },
  extraReducers: (build) => {
    build.addCase(
      respondToSignatureRequest.fulfilled,
      (state, { payload }) => {
        const { sigRequestId, signed, signature } = payload;
        sigRequestsAdapter.updateOne(state, {
          id: sigRequestId,
          changes: {
            signed,
            sigResult: signature || undefined
          }
        });
      }
    );
  },
});

export default slice.reducer;

export const { setSignatureRequests } = slice.actions;

export const sigRequestsSelectors = sigRequestsAdapter.getSelectors(
  (state: RootState) => state.signatureRequests
);