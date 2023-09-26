// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Yup from 'yup';
import { createTokenValidation } from '_shared/validation';

export function validate(...args: Parameters<typeof createTokenValidation>) {
	return Yup.object({
		amount: createTokenValidation(...args),
	});
}
