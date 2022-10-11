// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isValidSuiAddress } from '@mysten/sui.js';
import * as Yup from 'yup';

export const SUI_ADDRESS_VALIDATION = Yup.string()
    .ensure()
    .trim()
    .required()
    .transform((value: string) =>
        value.startsWith('0x') || value === '' || value === '0'
            ? value
            : `0x${value}`
    )
    .test(
        'is-sui-address',
        // eslint-disable-next-line no-template-curly-in-string
        'Invalid address. Please check again.',
        (value) => isValidSuiAddress(value)
    )
    .label("Recipient's address");
