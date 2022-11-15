// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Yup from 'yup';

import { SUI_ADDRESS_VALIDATION } from '_components/address-input/validation';
import { createTokenValidation } from '_src/shared/validation';

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
    decimals: number,
    gasDecimals: number,
    gasBudget: number
) {
    return Yup.object({
        amount: createTokenValidation(
            coinType,
            coinBalance,
            coinSymbol,
            gasBalance,
            decimals,
            gasDecimals,
            gasBudget
        ),
    });
}
