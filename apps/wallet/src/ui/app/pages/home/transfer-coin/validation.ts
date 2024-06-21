// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { createSuiAddressValidation } from '_components/address-input/validation';
import { createTokenValidation } from '_src/shared/validation';
import { type SuiClient } from '@mysten/sui/client';
import * as Yup from 'yup';

export function createValidationSchemaStepOne(
	client: SuiClient,
	suiNSEnabled: boolean,
	...args: Parameters<typeof createTokenValidation>
) {
	return Yup.object({
		to: createSuiAddressValidation(client, suiNSEnabled),
		amount: createTokenValidation(...args),
	});
}
