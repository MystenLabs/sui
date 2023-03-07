// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    Coin as CoinAPI,
    getEvents,
    getTransactionEffects,
    SUI_SYSTEM_STATE_OBJECT_ID,
    getObjectType,
} from '@mysten/sui.js';
import * as Sentry from '@sentry/react';

import type {
    ObjectId,
    SuiObjectData,
    SuiAddress,
    SuiMoveObject,
    SuiTransactionResponse,
    SignerWithProvider,
} from '@mysten/sui.js';

const COIN_TYPE = '0x2::coin::Coin';
const COIN_TYPE_ARG_REGEX = /^0x2::coin::Coin<(.+)>$/;

export const DEFAULT_GAS_BUDGET_FOR_PAY = 150;
export const DEFAULT_GAS_BUDGET_FOR_STAKE = 15000;
export const GAS_TYPE_ARG = '0x2::sui::SUI';
export const GAS_SYMBOL = 'SUI';
export const DEFAULT_NFT_TRANSFER_GAS_FEE = 450;

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
        coins: SuiMoveObject[],
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

    /**
     * Stake `amount` of Coin<T> to `validator`. Technically it means user delegates `amount` of Coin<T> to `validator`,
     * such that `validator` will stake the `amount` of Coin<T> for the user.
     *
     * @param signer A signer with connection to fullnode
     * @param coins A list of Coins owned by the signer with the same generic type(e.g., 0x2::Sui::Sui)
     * @param amount The amount to be staked
     * @param validator The sui address of the chosen validator
     */
    public static async stakeCoin(
        signer: SignerWithProvider,
        coins: SuiMoveObject[],
        amount: bigint,
        validator: SuiAddress,
        gasPrice: number
    ): Promise<SuiTransactionResponse> {
        const transaction = Sentry.startTransaction({ name: 'stake' });
        const stakeCoin = await this.coinManageForStake(
            signer,
            coins,
            amount,
            BigInt(gasPrice * DEFAULT_GAS_BUDGET_FOR_STAKE),
            transaction
        );

        const span = transaction.startChild({
            op: 'request-add-delegation',
            description: 'Staking move call',
        });

        try {
            return await signer.signAndExecuteTransaction({
                kind: 'moveCall',
                data: {
                    packageObjectId: '0x2',
                    module: 'sui_system',
                    function: 'request_add_delegation_mul_coin',
                    typeArguments: [],
                    arguments: [
                        SUI_SYSTEM_STATE_OBJECT_ID,
                        [stakeCoin],
                        [String(amount)],
                        validator,
                    ],
                    gasBudget: DEFAULT_GAS_BUDGET_FOR_STAKE,
                },
            });
        } finally {
            span.finish();
            transaction.finish();
        }
    }

    public static async unStakeCoin(
        signer: SignerWithProvider,
        delegation: ObjectId,
        stakedSuiId: ObjectId
    ): Promise<SuiTransactionResponse> {
        const transaction = Sentry.startTransaction({ name: 'unstake' });
        try {
            return await signer.signAndExecuteTransaction({
                kind: 'moveCall',
                data: {
                    packageObjectId: '0x2',
                    module: 'sui_system',
                    function: 'request_withdraw_delegation',
                    typeArguments: [],
                    arguments: [
                        SUI_SYSTEM_STATE_OBJECT_ID,
                        delegation,
                        stakedSuiId,
                    ],
                    gasBudget: DEFAULT_GAS_BUDGET_FOR_STAKE,
                },
            });
        } finally {
            transaction.finish();
        }
    }

    private static async coinManageForStake(
        signer: SignerWithProvider,
        coins: SuiMoveObject[],
        amount: bigint,
        gasFee: bigint,
        transaction: ReturnType<typeof Sentry['startTransaction']>
    ) {
        const span = transaction.startChild({
            op: 'coin-manage',
            description: 'Coin management for staking',
        });

        try {
            const totalAmount = amount + gasFee;
            const gasBudget = Coin.computeGasBudgetForPay(coins, totalAmount);
            const inputCoins =
                CoinAPI.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
                    coins,
                    totalAmount + BigInt(gasBudget)
                );

            const address = await signer.getAddress();

            const result = await signer.signAndExecuteTransaction({
                kind: 'paySui',
                data: {
                    // NOTE: We reverse the order here so that the highest coin is in the front
                    // so that it is used as the gas coin.
                    inputCoins: [...inputCoins]
                        .reverse()
                        .map((coin) => Coin.getID(coin as SuiMoveObject)),
                    recipients: [address, address],
                    // TODO: Update SDK to accept bigint
                    amounts: [Number(amount), Number(gasFee)],
                    gasBudget,
                },
            });

            const effects = getTransactionEffects(result);
            const events = getEvents(result);

            if (!effects || !events) {
                throw new Error('Missing effects or events');
            }

            const changeEvent = events.find((event) => {
                if ('coinBalanceChange' in event) {
                    return event.coinBalanceChange.amount === Number(amount);
                }

                return false;
            });

            if (!changeEvent || !('coinBalanceChange' in changeEvent)) {
                throw new Error('Missing coin balance event');
            }

            return changeEvent.coinBalanceChange.coinObjectId;
        } finally {
            span.finish();
        }
    }
}
