// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSuiAddressValidation } from '_components/address-input/validation';
import { type SuiClient } from '@mysten/sui/client';
import * as Yup from 'yup';

export function createValidationSchema(
	client: SuiClient,
	suiNSEnabled: boolean,
	senderAddress: string,
	objectId: string,
) {
	return Yup.object({
		to: createSuiAddressValidation(client, suiNSEnabled)
			.test(
				'sender-address',
				// eslint-disable-next-line no-template-curly-in-string
				`NFT is owned by this address`,
				(value) => senderAddress !== value,
			)
			.test(
				'nft-sender-address',
				// eslint-disable-next-line no-template-curly-in-string
				`NFT address must be different from receiver address`,
				(value) => objectId !== value,
			),
	});
}
