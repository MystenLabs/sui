// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import BigNumber from 'bignumber.js';
import * as Yup from 'yup';

import { formatBalance } from '../../hooks/useFormatCoin';

export function createValidationSchema(
    coinBalance: bigint,
    coinSymbol: string,
    decimals: number
) {
    return Yup.object({
        // NOTE: This is an intentional subset of the token validaiton:
        amount: Yup.mixed()
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
                    return amount
                        ? amount.shiftedBy(decimals).isInteger()
                        : false;
                }
            )
            .label('Amount'),
    });
}
