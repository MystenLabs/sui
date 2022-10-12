// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Yup from 'yup';

import { SUI_ADDRESS_VALIDATION } from '_components/address-input/validation';

export function createValidationSchema(
    gasBalance: bigint,
    senderAddress: string,
    objectId: string
) {
    return Yup.object({
        to: SUI_ADDRESS_VALIDATION.test(
            'sender-address',
            // eslint-disable-next-line no-template-curly-in-string
            `NFT is owned by this address`,
            (value) => senderAddress !== value
        ).test(
            'nft-sender-address',
            // eslint-disable-next-line no-template-curly-in-string
            `NFT address must be different from receiver address`,
            (value) => objectId !== value
        ),
        amount: Yup.number()
            .integer()
            .required()
            .test(
                'nft-gas-balance-check',
                `Insufficient balance to cover gas fee`,
                (amount) => {
                    return gasBalance >= BigInt(amount || 0);
                }
            )
            .label('Amount'),
    });
}
