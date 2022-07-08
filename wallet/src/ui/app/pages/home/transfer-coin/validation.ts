// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Yup from 'yup';

import { SUI_ADDRESS_VALIDATION } from '_components/address-input/validation';
import {
    DEFAULT_GAS_BUDGET_FOR_TRANSFER,
    GAS_TYPE_ARG,
    GAS_SYMBOL,
} from '_redux/slices/sui-objects/Coin';

import type { FormatNumberOptions, IntlShape } from 'react-intl';

export function createValidationSchema(
    coinType: string,
    coinBalance: bigint,
    coinSymbol: string,
    gasBalance: bigint,
    totalGasCoins: number,
    intl: IntlShape,
    formatOptions: FormatNumberOptions
) {
    return Yup.object({
        to: SUI_ADDRESS_VALIDATION,
        amount: Yup.number()
            .integer()
            .required()
            .min(
                1,
                `\${path} must be greater than or equal to \${min} ${coinSymbol}`
            )
            .test(
                'max',
                `\${path} must be less than or equal to ${intl.formatNumber(
                    coinBalance,
                    formatOptions
                )} ${coinSymbol}`,
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
                        // TODO: implement more sophisticated validation by taking
                        // the splitting/merging fee into account
                        return availableGas >= DEFAULT_GAS_BUDGET_FOR_TRANSFER;
                    } catch (e) {
                        return false;
                    }
                }
            )
            .test(
                'num-gas-coins-check',
                `Need at least 2 ${GAS_SYMBOL} coins to transfer a ${GAS_SYMBOL} coin`,
                () => {
                    return coinType !== GAS_TYPE_ARG || totalGasCoins >= 2;
                }
            )
            .label('Amount'),
    });
}
