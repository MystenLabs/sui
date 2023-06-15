// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Yup from 'yup';

import { normalizeMnemonics, validateMnemonics } from '_src/shared/utils/bip39';

export const mnemonicValidation = Yup.array()
	.of(Yup.string().ensure().trim())
	.transform((mnemonic: string[]) => normalizeMnemonics(mnemonic.join(' ')).split(' '))
	.test('mnemonic-valid', 'Recovery Passphrase is invalid', (mnemonic) => {
		return validateMnemonics(mnemonic?.join(' ') || '');
	})
	.label('Recovery Passphrase');
