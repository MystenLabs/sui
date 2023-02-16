// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { object } from 'yup';

import { SUI_ADDRESS_VALIDATION } from '_components/address-input/validation';
import { createTokenValidation } from '_src/shared/validation';

export function createValidationSchemaStepOne(
    ...args: Parameters<typeof createTokenValidation>
) {
    return object({
        to: SUI_ADDRESS_VALIDATION,
        amount: createTokenValidation(...args),
    });
}
