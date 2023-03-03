// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { formatBalance } from '@mysten/core';
import BigNumber from 'bignumber.js';
import * as Yup from 'yup';

import { GAS_SYMBOL, GAS_TYPE_ARG } from '_redux/slices/sui-objects/Coin';

export function createTokenValidation(
    coinType: string,
    coinBalance: bigint,
    coinSymbol: string,
    gasBalance: bigint,
    decimals: number,
    // TODO: We can move this to a constant when MIST is fully rolled out.
    gasDecimals: number,
    gasBudget: number | null,
    maxSuiSingleCoinBalance: bigint
) {
    return Yup.mixed()
        .transform((_, original) => {
            return new BigNumber(original);
        })
        .test('required', `\${path} is a required field`, (value) => {
            return !!value;
        })
        .test(
            'valid',
            'The value provided is not valid.',
            (value?: BigNumber) => {
                if (!value || value.isNaN() || !value.isFinite()) {
                    return false;
                }
                return true;
            }
        )
        .test(
            'min',
            `\${path} must be greater than 0 ${coinSymbol}`,
            (amount?: BigNumber) => (amount ? amount.gt(0) : false)
        )
        .test(
            'max',
            `\${path} must be less than ${formatBalance(
                coinBalance,
                decimals
            )} ${coinSymbol}`,
            (amount?: BigNumber) =>
                amount
                    ? amount.shiftedBy(decimals).lte(coinBalance.toString())
                    : false
        )
        .test(
            'max-decimals',
            `The value exceeds the maximum decimals (${decimals}).`,
            (amount?: BigNumber) => {
                return amount ? amount.shiftedBy(decimals).isInteger() : false;
            }
        )
        .test({
            name: 'gas-balance-check-enough-single-coin',
            test: function (_, ctx) {
                const gasBudgetInput = (ctx.parent?.gasInputBudgetEst ||
                    gasBudget ||
                    0) as number;
                // ignore gas budget check if gasBudget is null or gasInputBudgetEst is not null
                if (ctx.parent?.isPayAllSui && coinType === GAS_TYPE_ARG) {
                    return true;
                }

                if (
                    !!gasBudgetInput &&
                    maxSuiSingleCoinBalance >= gasBudgetInput
                ) {
                    return true;
                }

                return ctx.createError({
                    message: `Insufficient ${GAS_SYMBOL}, there is no individual coin with enough balance to cover for the gas fee (${formatBalance(
                        gasBudgetInput,
                        gasDecimals
                    )} ${GAS_SYMBOL})`,
                });
            },
        })

        .test({
            name: 'gas-balance-check',
            test: function (amount: BigNumber | undefined, ctx) {
                // For Pay All SUI and SUI coinType, we don't need to check gas balance.
                if (ctx.parent?.isPayAllSui && coinType === GAS_TYPE_ARG) {
                    return true;
                }

                const gasBudgetInput = (ctx.parent?.gasInputBudgetEst ||
                    gasBudget ||
                    0) as number;
                try {
                    let availableGas = gasBalance;
                    if (coinType === GAS_TYPE_ARG) {
                        availableGas -= BigInt(
                            amount?.shiftedBy(decimals).toString() || '0'
                        );
                    }
                    if (availableGas >= gasBudgetInput) {
                        return true;
                    }
                } catch (e) {
                    // ignore error
                }

                return ctx.createError({
                    message: `Insufficient ${GAS_SYMBOL} balance to cover gas fee (${formatBalance(
                        gasBudgetInput,
                        gasDecimals
                    )} ${GAS_SYMBOL})`,
                });
            },
        })

        .label('Amount');
}
