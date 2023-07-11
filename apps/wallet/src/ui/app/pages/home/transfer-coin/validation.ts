// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type JsonRpcProvider } from '@mysten/sui.js';
import * as Yup from 'yup';

import { createSuiAddressValidation } from '_components/address-input/validation';
import { createTokenValidation } from '_src/shared/validation';

export function createValidationSchemaStepOne(
	rpc: JsonRpcProvider,
	suiNSEnabled: boolean,
	...args: Parameters<typeof createTokenValidation>
) {
	return Yup.object({
		to: createSuiAddressValidation(rpc, suiNSEnabled),
		amount: createTokenValidation(...args),
	});
}
