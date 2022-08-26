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

export function createValidationSchemaStepTwo() {
    return Yup.object({
        to: SUI_ADDRESS_VALIDATION,
    });
}

export function createValidationSchemaStepOne(
    coinType: string,
    coinBalance: bigint,
    coinSymbol: string,
    gasBalance: bigint,
    totalGasCoins: number,
    intl: IntlShape,
    formatOptions: FormatNumberOptions
) {
    return Yup.object({
        amount: Yup.number()
            .integer()
            .required()
            .min(
                1,
                `\${path} must be greater than or equal to \${min} ${coinSymbol}`
            )
            .test(
                'max',
                `\${path} must be less than ${intl.formatNumber(
                    coinBalance,
                    formatOptions
                )} ${coinSymbol}`,
                (amount) =>
                    typeof amount === 'undefined' ||
                    BigInt(amount) <= coinBalance
            )
            .test(
                'gas-balance-check',
                `Insufficient ${GAS_SYMBOL} balance to cover gas fee (${DEFAULT_GAS_BUDGET_FOR_TRANSFER} ${GAS_SYMBOL})`,
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
            .label('Amount'),
    });
}
