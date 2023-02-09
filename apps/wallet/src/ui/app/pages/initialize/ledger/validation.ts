// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Yup from 'yup';

export const derivationPathValidation = Yup.string()
    .ensure()
    .required()
    .trim()
    //.transform((derivationPath) => normalizeMnemonics(derivationPath))
    .test(
        'derivationPath-valid',
        'Derivation Path is invalid',
        (derivationPath) =>
            //validateMnemonics(derivationPath)
            true
    )
    .label('Derivation Path');
