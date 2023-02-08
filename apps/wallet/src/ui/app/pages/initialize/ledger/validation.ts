// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Yup from 'yup';

import { normalizeMnemonics, validateMnemonics } from '_src/shared/utils/bip39';

export const mnemonicValidation = Yup.string()
    .ensure()
    .required()
    .trim()
    .transform((mnemonic) => normalizeMnemonics(mnemonic))
    .test('mnemonic-valid', 'Recovery Passphrase is invalid', (mnemonic) =>
        validateMnemonics(mnemonic)
    )
    .label('Recovery Passphrase');
