// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    isSuiMoveObject,
    getTransactionDigest,
    getTransactions,
    getTransactionKindName,
    getTransferCoinTransaction,
    getExecutionStatusType,
    getTotalGasUsed,
} from '@mysten/sui.js';
import {
    createAsyncThunk,
    createEntityAdapter,
    createSlice,
} from '@reduxjs/toolkit';

import {
    fetchAllOwnedObjects,
    suiObjectsAdapterSelectors,
} from '_redux/slices/sui-objects';
import { Coin } from '_redux/slices/sui-objects/Coin';

import type {
    SuiAddress,
    SuiMoveObject,
    TransactionEffectsResponse,
    GetTxnDigestsResponse,
    CertifiedTransaction,
} from '@mysten/sui.js';
import type { RootState } from '_redux/RootReducer';
import type { AppThunkConfig } from '_store/thunk-extras';

type SendTokensTXArgs = {
    tokenTypeArg: string;
    amount: bigint;
    recipientAddress: SuiAddress;
};
type TransactionResult = { EffectResponse: TransactionEffectsResponse };
type TxResultByAddress = [];

export const sendTokens = createAsyncThunk<
    TransactionResult,
    SendTokensTXArgs,
    AppThunkConfig
>(
    'sui-objects/send-tokens',
    async (
        { tokenTypeArg, amount, recipientAddress },
        { getState, extra: { api, keypairVault }, dispatch }
    ) => {
        const state = getState();
        const coinType = Coin.getCoinTypeFromArg(tokenTypeArg);
        const coins: SuiMoveObject[] = suiObjectsAdapterSelectors
            .selectAll(state)
            .filter(
                (anObj) =>
                    isSuiMoveObject(anObj.data) && anObj.data.type === coinType
            )
            .map(({ data }) => data as SuiMoveObject);
        const response = await Coin.transferCoin(
            api.getSignerInstance(keypairVault.getKeyPair()),
            coins,
            amount,
            recipientAddress
        );

        // TODO: better way to sync latest objects
        dispatch(fetchAllOwnedObjects());
        // TODO: is this correct? Find a better way to do it
        return response as TransactionResult;
    }
);

const deduplicate = (results: [number, string][] | undefined) =>
    results
        ? results
              .map((result) => result[1])
              .filter((value, index, self) => self.indexOf(value) === index)
        : [];

export const getTransactionsByAddress = createAsyncThunk<
    TxResultByAddress,
    GetTxnDigestsResponse,
    AppThunkConfig
>('sui-transactions/fetch-txData', async (_, { getState, extra: { api } }) => {
    const address = getState().account.address;

    if (address) {
        // Get all transactions txId for address
        const transactions: GetTxnDigestsResponse =
            await api.instance.getTransactionsForAddress(address);
        //getTransactionWithEffectsBatch

        const resp = await api.instance
            .getTransactionWithEffectsBatch(deduplicate(transactions))
            .then((txEffs: TransactionEffectsResponse[]) => {
                return (
                    txEffs
                        .map((txEff, i) => {
                            const [seq, digest] = transactions.filter(
                                (transactionId) =>
                                    transactionId[1] ===
                                    getTransactionDigest(txEff.certificate)
                            )[0];
                            const res: CertifiedTransaction = txEff.certificate;
                            // TODO: handle multiple transactions
                            const txns = getTransactions(res);
                            if (txns.length > 1) {
                                return null;
                            }
                            const txn = txns[0];
                            const txKind = getTransactionKindName(txn);
                            const recipient =
                                getTransferCoinTransaction(txn)?.recipient;

                            return {
                                seq,
                                txId: digest,
                                status: getExecutionStatusType(txEff),
                                txGas: getTotalGasUsed(txEff),
                                kind: txKind,
                                From: res.data.sender,
                                ...(recipient
                                    ? {
                                          To: recipient,
                                      }
                                    : {}),
                            };
                        })
                        // Remove failed transactions and sort by sequence number
                        .filter((itm) => itm)
                        .sort((a, b) => b!.seq - a!.seq)
                );
            });
        return resp as any;
    }
});

const txAdapter = createEntityAdapter<TransactionResult>({
    selectId: (tx) => tx.EffectResponse.certificate.transactionDigest,
});

export const txSelectors = txAdapter.getSelectors(
    (state: RootState) => state.transactions
);

const slice = createSlice({
    name: 'transactions',
    initialState: txAdapter.getInitialState(),
    reducers: {},
    extraReducers: (builder) => {
        builder.addCase(sendTokens.fulfilled, (state, { payload }) => {
            return txAdapter.setOne(state, payload);
        });
    },
});

export default slice.reducer;
