// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SUI_TYPE_ARG, Coin as CoinAPI } from '@mysten/sui.js';
import { useMutation } from '@tanstack/react-query';

import { useSigner } from '_hooks';

import type { SuiAddress, SuiMoveObject } from '@mysten/sui.js';

type SendTokensTXArgs = {
    tokenTypeArg: string;
    amount: bigint;
    recipientAddress: SuiAddress;
    gasBudget: number;
    sendMax: boolean;
    coins: SuiMoveObject[];
};

export function useExecuteCoinTransfer({
    tokenTypeArg,
    amount,
    recipientAddress,
    gasBudget,
    sendMax,
    coins,
}: SendTokensTXArgs) {
    const signer = useSigner();

    return useMutation({
        mutationFn: async () => {
            if (!signer) throw new Error('Signer not found');

            let response;
            // Use payAllSui if sendMax is true and the token type is SUI
            if (sendMax && tokenTypeArg === SUI_TYPE_ARG) {
                response = await signer.payAllSui({
                    recipient: recipientAddress,
                    gasBudget: gasBudget,
                    inputCoins: coins.map((coin) => CoinAPI.getID(coin)),
                });
            } else {
                response = await signer.signAndExecuteTransaction(
                    await CoinAPI.newPayTransaction(
                        coins,
                        tokenTypeArg,
                        amount,
                        recipientAddress,
                        gasBudget
                    )
                );
            }
            return response;
        },
    });
}
