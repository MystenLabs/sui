// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin, COIN_DENOMINATIONS } from '@mysten/sui.js';
import * as Yup from 'yup';

import { coinFormat } from '_app/shared/coin-balance/coin-format';
import {
    DEFAULT_GAS_BUDGET_FOR_STAKE,
    GAS_TYPE_ARG,
    GAS_SYMBOL,
} from '_redux/slices/sui-objects/Coin';

import type { IntlShape } from 'react-intl';

export function createValidationSchema(
    coinType: string,
    coinBalance: bigint,
    gasBalance: bigint,
    totalGasCoins: number,
    intl: IntlShape
) {
    const minValue = BigInt(1);
    const balanceFormatted = coinFormat(
        intl,
        coinBalance,
        coinType,
        'accurate'
    ).displayFull;
    const minFormatted = coinFormat(
        intl,
        minValue,
        coinType,
        'accurate'
    ).displayFull;
    // this should be provided by the input component but for now we only select sui
    // TODO: get denomination from the input component
    const denomination =
        coinType in COIN_DENOMINATIONS ? COIN_DENOMINATIONS[GAS_TYPE_ARG] : 1;
    return Yup.object({
        amount: Yup.number()
            .required()
            .transform((_, original) =>
                Number(Coin.fromInput(original, denomination))
            )
            .min(
                Number(minValue),
                `\${path} must be greater than or equal to ${minFormatted}`
            )
            .test(
                'max',
                `\${path} must be less than or equal to ${balanceFormatted}`,
                (amount) =>
                    typeof amount === 'undefined' ||
                    BigInt(amount) <= coinBalance
            )
            .test(
                'gas-balance-check',
                `Insufficient ${GAS_SYMBOL} balance to cover gas fee`,
                (amount) => {
                    try {
                        let availableGas = gasBalance;
                        if (coinType === GAS_TYPE_ARG) {
                            availableGas -= BigInt(amount || 0);
                        }
                        return availableGas >= DEFAULT_GAS_BUDGET_FOR_STAKE;
                    } catch (e) {
                        return false;
                    }
                }
            )
            .test(
                'num-gas-coins-check',
                `Need at least 2 ${GAS_SYMBOL} coins to stake a ${GAS_SYMBOL} coin`,
                () => {
                    return coinType !== GAS_TYPE_ARG || totalGasCoins >= 2;
                }
            )
            .label('Amount'),
    });
}
