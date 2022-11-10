// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isSuiMoveObject, Coin as CoinAPI, SUI_TYPE_ARG } from '@mysten/sui.js';

import type {
    ObjectId,
    SuiObject,
    SuiMoveObject,
    RawSigner,
    SuiAddress,
    JsonRpcProvider,
    PayTransaction,
    SuiExecuteTransactionResponse,
} from '@mysten/sui.js';

const COIN_TYPE = '0x2::coin::Coin';
const COIN_TYPE_ARG_REGEX = /^0x2::coin::Coin<(.+)>$/;
export const DEFAULT_GAS_BUDGET_FOR_SPLIT = 10000;
export const DEFAULT_GAS_BUDGET_FOR_MERGE = 10000;
export const DEFAULT_GAS_BUDGET_FOR_TRANSFER = 100;
export const DEFAULT_GAS_BUDGET_FOR_TRANSFER_SUI = 100;
export const DEFAULT_GAS_BUDGET_FOR_PAY = 150;
export const DEFAULT_GAS_BUDGET_FOR_STAKE = 10000;
export const GAS_TYPE_ARG = '0x2::sui::SUI';
export const GAS_SYMBOL = 'SUI';
export const DEFAULT_NFT_TRANSFER_GAS_FEE = 450;
export const SUI_SYSTEM_STATE_OBJECT_ID =
    '0x0000000000000000000000000000000000000005';

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
     * @param signer A signer with connection to fullnode
     * @param coins A list of Coins owned by the signer with the same generic type(e.g., 0x2::Sui::Sui)
     * @param amount The amount to be transfer
     * @param recipient The sui address of the recipient
     */
    public static async transferCoin(
        signer: RawSigner,
        coins: SuiMoveObject[],
        amount: bigint,
        recipient: SuiAddress
    ): Promise<SuiExecuteTransactionResponse> {
        const inputCoins =
            await CoinAPI.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
                coins,
                amount
            );
        if (inputCoins.length === 0) {
            const totalBalance = CoinAPI.totalBalance(coins);
            throw new Error(
                `Coin balance ${totalBalance.toString()} is not sufficient to cover the transfer amount ` +
                    `${amount.toString()}. Try reducing the transfer amount to ${totalBalance}.`
            );
        }

        const inputCoinIDs = inputCoins.map((c) => CoinAPI.getID(c));
        const gasBudget = Coin.computeGasCostForPay(inputCoins.length);
        const payTxn: PayTransaction = {
            inputCoins: inputCoinIDs,
            recipients: [recipient],
            amounts: [Number(amount)],
            gasBudget,
            gasPayment: await Coin.selectGasPayment(
                coins,
                inputCoinIDs,
                BigInt(gasBudget)
            ),
        };
        return await signer.pay(payTxn);
    }

    private static computeGasCostForPay(numInputCoins: number): number {
        // TODO: improve the gas budget estimation
        return (
            DEFAULT_GAS_BUDGET_FOR_PAY *
            Math.max(2, Math.min(100, numInputCoins / 2))
        );
    }

    private static async selectGasPayment(
        coins: SuiMoveObject[],
        exclude: ObjectId[],
        amount: bigint
    ): Promise<ObjectId> {
        const gasPayment =
            await CoinAPI.selectCoinWithBalanceGreaterThanOrEqual(
                coins,
                amount,
                exclude
            );
        if (gasPayment === undefined) {
            throw new Error(
                `Unable to find a coin to cover the gas budget ${amount.toString()}`
            );
        }
        return CoinAPI.getID(gasPayment);
    }

    /**
     * Transfer `amount` of Coin<Sui> to `recipient`.
     *
     * @param signer A signer with connection to fullnode
     * @param coins A list of Sui Coins owned by the signer
     * @param amount The amount to be transferred
     * @param recipient The sui address of the recipient
     */
    public static async transferSui(
        signer: RawSigner,
        coins: SuiMoveObject[],
        amount: bigint,
        recipient: SuiAddress
    ): Promise<SuiExecuteTransactionResponse> {
        const targetAmount =
            amount + BigInt(DEFAULT_GAS_BUDGET_FOR_TRANSFER_SUI);
        const coinsWithSufficientAmount =
            await CoinAPI.selectCoinsWithBalanceGreaterThanOrEqual(
                coins,
                targetAmount
            );
        if (coinsWithSufficientAmount.length > 0) {
            const txn = {
                suiObjectId: CoinAPI.getID(coinsWithSufficientAmount[0]),
                gasBudget: DEFAULT_GAS_BUDGET_FOR_TRANSFER_SUI,
                recipient: recipient,
                amount: Number(amount),
            };
            return await signer.transferSui(txn);
        }

        // TODO: use PaySui Transaction when it is ready
        // If there is not a coin with sufficient balance, use the pay API
        const gasCostForPay = Coin.computeGasCostForPay(coins.length);
        let inputCoins = await Coin.assertAndGetCoinsWithBalanceGte(
            coins,
            amount,
            gasCostForPay
        );

        // In this case, all coins are needed to cover the transfer amount plus gas budget, leaving
        // no coins for gas payment. This won't be a problem once we introduce `PaySui`. But for now,
        // we address this case by splitting an extra coin.
        if (inputCoins.length === coins.length) {
            // We need to pay for an additional `transferSui` transaction now, assert that we have sufficient balance
            // to cover the additional cost
            await Coin.assertAndGetCoinsWithBalanceGte(
                coins,
                amount,
                gasCostForPay + DEFAULT_GAS_BUDGET_FOR_TRANSFER_SUI
            );

            // Split the gas budget from the coin with largest balance for simplicity. We can also use any coin
            // that has amount greater than or equal to `DEFAULT_GAS_BUDGET_FOR_TRANSFER_SUI * 2`
            const coinWithLargestBalance = inputCoins[inputCoins.length - 1];

            if (
                // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
                CoinAPI.getBalance(coinWithLargestBalance)! <
                gasCostForPay + DEFAULT_GAS_BUDGET_FOR_TRANSFER_SUI
            ) {
                throw new Error(
                    `None of the coins has sufficient balance to cover gas fee`
                );
            }

            const txn = {
                suiObjectId: CoinAPI.getID(coinWithLargestBalance),
                gasBudget: DEFAULT_GAS_BUDGET_FOR_TRANSFER_SUI,
                recipient: await signer.getAddress(),
                amount: gasCostForPay,
            };
            await signer.transferSui(txn);

            inputCoins =
                await signer.provider.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
                    await signer.getAddress(),
                    amount,
                    SUI_TYPE_ARG,
                    []
                );
        }
        const txn = {
            inputCoins: inputCoins.map((c) => CoinAPI.getID(c)),
            recipients: [recipient],
            amounts: [Number(amount)],
            gasBudget: gasCostForPay,
        };
        return await signer.pay(txn);
    }

    private static async assertAndGetCoinsWithBalanceGte(
        coins: SuiMoveObject[],
        amount: bigint,
        gasBudget?: number
    ) {
        const inputCoins =
            await CoinAPI.selectCoinSetWithCombinedBalanceGreaterThanOrEqual(
                coins,
                amount + BigInt(gasBudget ?? 0)
            );
        if (inputCoins.length === 0) {
            const totalBalance = CoinAPI.totalBalance(coins);
            const maxTransferAmount = totalBalance - BigInt(gasBudget ?? 0);
            const gasText = gasBudget ? ` plus gas budget ${gasBudget}` : '';
            throw new Error(
                `Coin balance ${totalBalance.toString()} is not sufficient to cover the transfer amount ` +
                    `${amount.toString()}${gasText}. ` +
                    `Try reducing the transfer amount to ${maxTransferAmount.toString()}.`
            );
        }
        return inputCoins;
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
        signer: RawSigner,
        coins: SuiMoveObject[],
        amount: bigint,
        validator: SuiAddress
    ): Promise<SuiExecuteTransactionResponse> {
        const coin = await Coin.requestSuiCoinWithExactAmount(
            signer,
            coins,
            amount
        );
        const txn = {
            packageObjectId: '0x2',
            module: 'sui_system',
            function: 'request_add_delegation',
            typeArguments: [],
            arguments: [SUI_SYSTEM_STATE_OBJECT_ID, coin, validator],
            gasBudget: DEFAULT_GAS_BUDGET_FOR_STAKE,
        };
        return await signer.executeMoveCall(txn);
    }

    private static async requestSuiCoinWithExactAmount(
        signer: RawSigner,
        coins: SuiMoveObject[],
        amount: bigint
    ): Promise<ObjectId> {
        const coinWithExactAmount = await Coin.selectSuiCoinWithExactAmount(
            signer,
            coins,
            amount
        );
        if (coinWithExactAmount) {
            return coinWithExactAmount;
        }
        // use transferSui API to get a coin with the exact amount
        await Coin.transferSui(
            signer,
            coins,
            amount,
            await signer.getAddress()
        );

        const coinWithExactAmount2 = await Coin.selectSuiCoinWithExactAmount(
            signer,
            coins,
            amount,
            true
        );
        if (!coinWithExactAmount2) {
            throw new Error(`requestCoinWithExactAmount failed unexpectedly`);
        }
        return coinWithExactAmount2;
    }

    private static async selectSuiCoinWithExactAmount(
        signer: RawSigner,
        coins: SuiMoveObject[],
        amount: bigint,
        refreshData = false
    ): Promise<ObjectId | undefined> {
        const coinsWithSufficientAmount = refreshData
            ? await signer.provider.selectCoinsWithBalanceGreaterThanOrEqual(
                  await signer.getAddress(),
                  amount,
                  SUI_TYPE_ARG,
                  []
              )
            : await CoinAPI.selectCoinsWithBalanceGreaterThanOrEqual(
                  coins,
                  amount
              );

        if (
            coinsWithSufficientAmount.length > 0 &&
            // eslint-disable-next-line @typescript-eslint/no-non-null-assertion
            CoinAPI.getBalance(coinsWithSufficientAmount[0])! === amount
        ) {
            return CoinAPI.getID(coinsWithSufficientAmount[0]);
        }

        return undefined;
    }

    public static async getActiveValidators(
        provider: JsonRpcProvider
    ): Promise<Array<SuiMoveObject>> {
        const contents = await provider.getObject(SUI_SYSTEM_STATE_OBJECT_ID);
        const data = (contents.details as SuiObject).data;
        const validators = (data as SuiMoveObject).fields.validators;
        const active_validators = (validators as SuiMoveObject).fields
            .active_validators;
        return active_validators as Array<SuiMoveObject>;
    }
}
