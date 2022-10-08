// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    passwordValidation,
    getConfirmPasswordValidation,
} from '_app/shared/input/password/validation';
import { WALLET_ENCRYPTION_ENABLED } from '_app/wallet/constants';

export const passwordFieldsValidation = WALLET_ENCRYPTION_ENABLED
    ? {
          password: passwordValidation,
          confirmPassword: getConfirmPasswordValidation('password'),
      }
    : ({} as Record<string, never>);
