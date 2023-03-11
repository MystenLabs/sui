// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    Coin as CoinAPI,
    SUI_SYSTEM_STATE_OBJECT_ID,
    getObjectType,
    Transaction,
} from '@mysten/sui.js';
import * as Sentry from '@sentry/react';

import type {
    ObjectId,
    SuiObjectData,
    SuiAddress,
    SuiMoveObject,
    SuiTransactionResponse,
    SignerWithProvider,
    CoinStruct,
} from '@mysten/sui.js';

const COIN_TYPE = '0x2::coin::Coin';
const COIN_TYPE_ARG_REGEX = /^0x2::coin::Coin<(.+)>$/;

export const DEFAULT_GAS_BUDGET_FOR_PAY = 150;
export const DEFAULT_GAS_BUDGET_FOR_STAKE = 15000;
export const GAS_TYPE_ARG = '0x2::sui::SUI';
export const GAS_SYMBOL = 'SUI';
export const DEFAULT_NFT_TRANSFER_GAS_FEE = 450;
export const DEFAULT_MINT_NFT_GAS_BUDGET = 2000;

// TODO use sdk
export class Coin {
    public static isCoin(obj: SuiObjectData) {
        return getObjectType(obj)?.startsWith(COIN_TYPE) ?? false;
    }

    public static getCoinTypeArg(obj: SuiMoveObject) {
        const res = obj.type.match(COIN_TYPE_ARG_REGEX);
        return res ? res[1] : null;
    }

    public static isSUI(obj: SuiMoveObject) {
        const arg = Coin.getCoinTypeArg(obj);
        return arg ? Coin.getCoinSymbol(arg) === 'SUI' : false;
    }

    public static getCoinSymbol(coinTypeArg: string) {
        return coinTypeArg.substring(coinTypeArg.lastIndexOf(':') + 1);
    }

    public static getBalance(obj: SuiMoveObject): bigint {
        return BigInt(obj.fields.balance);
    }

    public static getID(obj: SuiMoveObject): ObjectId {
        return obj.fields.id.id;
    }

    public static getCoinTypeFromArg(coinTypeArg: string) {
        return `${COIN_TYPE}<${coinTypeArg}>`;
    }

    public static computeGasBudgetForPay(
        coins: CoinStruct[],
        amountToSend: bigint
    ): number {
        // TODO: improve the gas budget estimation
        const numInputCoins =
            CoinAPI.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
                coins,
                amountToSend
            ).length;
        return (
            DEFAULT_GAS_BUDGET_FOR_PAY *
            Math.max(2, Math.min(100, numInputCoins / 2))
        );
    }

    // TODO: we should replace this function with the SDK implementation
    /**
     * Stake `amount` of Coin<T> to `validator`. Technically it means user stakes `amount` of Coin<T> to `validator`,
     * such that `validator` will stake the `amount` of Coin<T> for the user.
     *
     * @param signer A signer with connection to fullnode
     * @param coins A list of Coins owned by the signer with the same generic type(e.g., 0x2::Sui::Sui)
     * @param amount The amount to be staked
     * @param validator The sui address of the chosen validator
     */
    public static async stakeCoin(
        signer: SignerWithProvider,
        amount: bigint,
        validator: SuiAddress
    ): Promise<SuiTransactionResponse> {
        const transaction = Sentry.startTransaction({ name: 'stake' });

        const span = transaction.startChild({
            op: 'request-add-stake',
            description: 'Staking move call',
        });

        try {
            const tx = new Transaction();
            tx.setGasBudget(DEFAULT_GAS_BUDGET_FOR_STAKE);
            const stakeCoin = tx.add(
                Transaction.Commands.SplitCoin(tx.gas, tx.pure(amount))
            );
            tx.add(
                Transaction.Commands.MoveCall({
                    target: '0x2::sui_system::request_add_stake',
                    arguments: [
                        tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
                        stakeCoin,
                        tx.pure(validator),
                    ],
                })
            );
            return await signer.signAndExecuteTransaction(tx);
        } finally {
            span.finish();
            transaction.finish();
        }
    }

    public static async unStakeCoin(
        signer: SignerWithProvider,
        stake: ObjectId,
        stakedSuiId: ObjectId
    ): Promise<SuiTransactionResponse> {
        const transaction = Sentry.startTransaction({ name: 'unstake' });
        try {
            const tx = new Transaction();
            tx.setGasBudget(DEFAULT_GAS_BUDGET_FOR_STAKE);
            tx.add(
                Transaction.Commands.MoveCall({
                    target: '0x2::sui_system::request_withdraw_stake',
                    arguments: [
                        tx.object(SUI_SYSTEM_STATE_OBJECT_ID),
                        tx.object(stake),
                        tx.object(stakedSuiId),
                    ],
                })
            );
            return await signer.signAndExecuteTransaction(tx);
        } finally {
            transaction.finish();
        }
    }
}
