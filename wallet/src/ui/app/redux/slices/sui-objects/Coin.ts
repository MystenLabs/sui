// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getCoinAfterMerge,
    getMoveObject,
    isSuiMoveObject,
} from '@mysten/sui.js';

import type {
    ObjectId,
    SuiObject,
    SuiMoveObject,
    TransactionResponse,
    RawSigner,
    SuiAddress,
} from '@mysten/sui.js';

const COIN_TYPE = '0x2::coin::Coin';
const COIN_TYPE_ARG_REGEX = /^0x2::coin::Coin<(.+)>$/;
export const DEFAULT_GAS_BUDGET_FOR_SPLIT = 1000;
export const DEFAULT_GAS_BUDGET_FOR_MERGE = 500;
export const DEFAULT_GAS_BUDGET_FOR_TRANSFER = 100;
export const GAS_TYPE_ARG = '0x2::sui::SUI';
export const GAS_SYMBOL = 'SUI';

// TODO use sdk
export class Coin {
    public static isCoin(obj: SuiObject) {
        return isSuiMoveObject(obj.data) && obj.data.type.startsWith(COIN_TYPE);
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

    /**
     * Transfer `amount` of Coin<T> to `recipient`.
     *
     * @param signer A signer with connection to the gateway:e.g., new RawSigner(keypair, new JsonRpcProvider(endpoint))
     * @param coins A list of Coins owned by the signer with the same generic type(e.g., 0x2::Sui::Sui)
     * @param amount The amount to be transfer
     * @param recipient The sui address of the recipient
     */
    public static async transferCoin(
        signer: RawSigner,
        coins: SuiMoveObject[],
        amount: bigint,
        recipient: SuiAddress
    ): Promise<TransactionResponse> {
        const coin = await Coin.selectCoin(signer, coins, amount);
        return await signer.publicTransferObject({
            objectId: coin,
            gasBudget: DEFAULT_GAS_BUDGET_FOR_TRANSFER,
            recipient: recipient,
        });
    }

    private static async selectCoin(
        signer: RawSigner,
        coins: SuiMoveObject[],
        amount: bigint
    ): Promise<ObjectId> {
        const coin = await Coin.selectCoinForSplit(signer, coins, amount);
        const coinID = Coin.getID(coin);
        const balance = Coin.getBalance(coin);
        if (balance === amount) {
            return coinID;
        } else if (balance > amount) {
            await signer.splitCoin({
                coinObjectId: coinID,
                gasBudget: DEFAULT_GAS_BUDGET_FOR_SPLIT,
                splitAmounts: [Number(balance - amount)],
            });
            return coinID;
        } else {
            throw new Error(`Insufficient balance`);
        }
    }

    private static async selectCoinForSplit(
        signer: RawSigner,
        coins: SuiMoveObject[],
        amount: bigint
    ): Promise<SuiMoveObject> {
        // Sort coins by balance in an ascending order
        coins.sort((a, b) =>
            Coin.getBalance(a) - Coin.getBalance(b) > 0 ? 1 : -1
        );

        // return the coin with the smallest balance that is greater than or equal to the amount
        const coinWithSufficientBalance = coins.find(
            (c) => Coin.getBalance(c) >= amount
        );
        if (coinWithSufficientBalance) {
            return coinWithSufficientBalance;
        }

        // merge coins to have a coin with sufficient balance
        // we will start from the coins with the largest balance
        // and end with the coin with the second smallest balance(i.e., i > 0 instead of i >= 0)
        // we cannot merge coins with the smallest balance because we
        // need to have a separate coin to pay for the gas
        // TODO: there's some edge cases here. e.g., the total balance is enough before spliting/merging
        // but not enough if we consider the cost of splitting and merging.
        let primaryCoin = coins[coins.length - 1];
        for (let i = coins.length - 2; i > 0; i--) {
            const mergeTxn = await signer.mergeCoin({
                primaryCoin: Coin.getID(primaryCoin),
                coinToMerge: Coin.getID(coins[i]),
                gasBudget: DEFAULT_GAS_BUDGET_FOR_MERGE,
            });
            // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
            primaryCoin = getMoveObject(getCoinAfterMerge(mergeTxn)!)!;
            if (Coin.getBalance(primaryCoin) >= amount) {
                return primaryCoin;
            }
        }
        // primary coin might have a balance smaller than the `amount`
        return primaryCoin;
    }
}
