// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';
import { mixed, object } from 'yup';

import { formatBalance } from '../../../hooks/useFormatCoin';
import { SUI_ADDRESS_VALIDATION } from '_components/address-input/validation';

export function createValidationSchema(
    coinBalance: bigint,
    coinSymbol: string,
    decimals: number
) {
    return object({
        to: SUI_ADDRESS_VALIDATION,
        amount: mixed()
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
            .test('max', (amount: BigNumber | undefined, ctx) => {
                const gasBudget = ctx.parent.sendMaxToken
                    ? 0n
                    : ctx.parent.gasBudget || 0n;
                const availableBalance = coinBalance - gasBudget;
                if (availableBalance < 0) {
                    return ctx.createError({
                        message: 'Insufficient funds',
                    });
                }
                const enoughBalance = amount
                    ? amount
                          .shiftedBy(decimals)
                          .lte(availableBalance.toString())
                    : false;
                if (enoughBalance) {
                    return true;
                }
                return ctx.createError({
                    message: `\${path} must be less than ${formatBalance(
                        availableBalance,
                        decimals
                    )} ${coinSymbol}`,
                });
            })
            .test(
                'max-decimals',
                `The value exceeds the maximum decimals (${decimals}).`,
                (amount?: BigNumber) => {
                    return amount
                        ? amount.shiftedBy(decimals).isInteger()
                        : false;
                }
            )
            .label('Amount'),
        gasBudget: mixed()
            .transform((_, original) => {
                try {
                    return BigInt(original);
                } catch (e) {
                    return null;
                }
            })

            .test('required', '', (value) => {
                return !!value;
            })
            .test('gasBudget', (gasBudget, ctx) => {
                //NOTE: no need to include the amount because budget is included in the max check of the amount
                if (coinBalance > gasBudget || !gasBudget) {
                    return true;
                }
                return ctx.createError({
                    message: `Insufficient SUI balance (${formatBalance(
                        coinBalance,
                        decimals
                    )}) to cover for the gas fee (${formatBalance(
                        gasBudget,
                        decimals
                    )})`,
                });
            }),
    });
}
