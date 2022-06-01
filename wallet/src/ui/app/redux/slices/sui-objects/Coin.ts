// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject } from '@mysten/sui.js';

import type {
    ObjectId,
    SuiObject,
    SuiMoveObject,
    TransactionResponse,
    RawSigner,
    SuiAddress,
} from '@mysten/sui.js';

const COIN_TYPE = '0x2::Coin::Coin';
const COIN_TYPE_ARG_REGEX = /^0x2::Coin::Coin<(.+)>$/;
export const GAS_TYPE_ARG = '0x2::SUI::SUI';
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

    public static getCoinSymbol(coinTypeArg: string) {
        return coinTypeArg.substring(coinTypeArg.lastIndexOf(':') + 1);
    }

    public static getBalance(obj: SuiMoveObject) {
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
        amount: BigInt,
        recipient: SuiAddress
    ): Promise<TransactionResponse> {
        if (coins.length < 2) {
            throw new Error(`Not enough coins to transfer`);
        }
        const coin = await Coin.selectCoin(coins, amount);
        return await signer.transferCoin({
            objectId: coin,
            gasBudget: 1000,
            recipient: recipient,
        });
    }

    private static async selectCoin(
        coins: SuiMoveObject[],
        amount: BigInt
    ): Promise<ObjectId> {
        const coin = await Coin.selectCoinForSplit(coins, amount);
        // TODO: Split coin not implemented yet
        return Coin.getID(coin);
    }

    private static async selectCoinForSplit(
        coins: SuiMoveObject[],
        amount: BigInt
    ): Promise<SuiMoveObject> {
        // Sort coins by balance in an ascending order
        coins.sort();

        const coinWithSufficientBalance = coins.find(
            (c) => Coin.getBalance(c) >= amount
        );
        if (coinWithSufficientBalance) {
            return coinWithSufficientBalance;
        }

        // merge coins to have a coin with sufficient balance
        throw new Error(`Merge coin Not implemented`);
    }
}
