// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { isValidSuiAddress } from '@mysten/sui.js';
import * as Yup from 'yup';

export const SUI_ADDRESS_VALIDATION = Yup.string()
    .ensure()
    .trim()
    .required()
    .test(
        'is-sui-address',
        // eslint-disable-next-line no-template-curly-in-string
        'Invalid address. Please check again.',
        (value) => isValidSuiAddress(value)
    )
    .label("Recipient's address");
