// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Base64DataBuffer, type SignaturePubkeyPair } from '@mysten/sui.js';
import {
  createAsyncThunk,
  createEntityAdapter,
  createSlice,
  type PayloadAction
} from '@reduxjs/toolkit';

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
    signature: SignaturePubkeyPair | null;
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
    let sigResult: SignaturePubkeyPair | undefined = undefined;
    let sigResultError: string | undefined;
    if (signed) {
      const signer = api.getSignerInstance(keypairVault.getKeyPair());
      try {
        sigResult = await signer.signMessage(new Base64DataBuffer(sigRequest.message));
      } catch (err) {
        sigResultError = (err as Error).message;
      }
    }
    background.sendSignatureResponse(
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
            sigResult: signature?.signature.getData() || undefined
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