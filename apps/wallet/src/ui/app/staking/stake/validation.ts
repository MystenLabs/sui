// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Yup from 'yup';

import { createTokenValidation } from '_src/shared/validation';

export function createValidationSchema(
    coinType: string,
    coinBalance: bigint,
    coinSymbol: string,
    gasBalance: bigint,
    decimals: number,
    gasDecimals: number,
    gasBudget: number,
    maxSuiSingleCoinBalance: bigint
) {
    return Yup.object({
        amount: createTokenValidation(
            coinType,
            coinBalance,
            coinSymbol,
            gasBalance,
            decimals,
            gasDecimals,
            gasBudget,
            maxSuiSingleCoinBalance
        ),
    });
}
