// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Yup from 'yup';

import { passwordFieldsValidation } from '_pages/initialize/shared/password-fields/validation';

export const createMnemonicValidation = Yup.object({
	...{ terms: Yup.boolean().required().is([true]) },
	...passwordFieldsValidation,
});
