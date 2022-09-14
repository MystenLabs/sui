// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Coin, COIN_DENOMINATIONS } from '@mysten/sui.js';
import * as Yup from 'yup';

import { coinFormat } from '_app/shared/coin-balance/coin-format';
import { SUI_ADDRESS_VALIDATION } from '_components/address-input/validation';
import {
    DEFAULT_GAS_BUDGET_FOR_TRANSFER,
    GAS_TYPE_ARG,
    GAS_SYMBOL,
} from '_redux/slices/sui-objects/Coin';

import type { IntlShape } from 'react-intl';

export function createValidationSchemaStepTwo() {
    return Yup.object({
        to: SUI_ADDRESS_VALIDATION,
    });
}

export function createValidationSchemaStepOne(
    coinType: string,
    coinBalance: bigint,
    gasBalance: bigint,
    intl: IntlShape
) {
    // this should be provided by the input component but for now we only select sui
    // TODO: get denomination from the input component
    const denomination =
        coinType in COIN_DENOMINATIONS ? COIN_DENOMINATIONS[coinType] : 1;
    const minValue = BigInt(1);
    const minFormatted = coinFormat(
        intl,
        minValue,
        coinType,
        'accurate'
    ).displayFull;
    const balanceFormatted = coinFormat(
        intl,
        coinBalance,
        coinType,
        'accurate'
    ).displayFull;
    const gasCostFormatted = coinFormat(
        intl,
        BigInt(DEFAULT_GAS_BUDGET_FOR_TRANSFER),
        GAS_TYPE_ARG,
        'accurate'
    ).displayFull;
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
                `\${path} must be less than ${balanceFormatted}`,
                (amount) =>
                    typeof amount === 'undefined' ||
                    BigInt(amount) <= coinBalance
            )
            .test(
                'gas-balance-check',
                `Insufficient ${GAS_SYMBOL} balance to cover gas fee (${gasCostFormatted})`,
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
