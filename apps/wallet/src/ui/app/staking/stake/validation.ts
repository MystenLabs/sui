// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Yup from 'yup';

import {
    GAS_TYPE_ARG,
    GAS_SYMBOL,
    DEFAULT_GAS_BUDGET_FOR_STAKE,
} from '_redux/slices/sui-objects/Coin';
import { createTokenValidation } from '_src/shared/validation';

export function createValidationSchema(
    coinType: string,
    coinBalance: bigint,
    coinSymbol: string,
    gasBalance: bigint,
    totalGasCoins: number,
    decimals: number,
    gasDecimals: number
) {
    return Yup.object({
        amount: createTokenValidation(
            coinType,
            coinBalance,
            coinSymbol,
            gasBalance,
            decimals,
            gasDecimals,
            DEFAULT_GAS_BUDGET_FOR_STAKE
        ).test(
            'num-gas-coins-check',
            `Need at least 2 ${GAS_SYMBOL} coins to stake a ${GAS_SYMBOL} coin`,
            () => {
                return coinType !== GAS_TYPE_ARG || totalGasCoins >= 2;
            }
        ),
    });
}
