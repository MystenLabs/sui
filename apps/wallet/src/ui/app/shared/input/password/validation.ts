// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Yup from 'yup';

function containsUpperCaseLetters(str: string) {
    for (const letter of str) {
        if (
            letter.toLowerCase() !== letter.toUpperCase() &&
            letter === letter.toUpperCase()
        ) {
            return true;
        }
    }
    return false;
}

export const passwordValidation = Yup.string()
    .ensure()
    .required()
    .min(8)
    .test({
        name: 'password-strength',
        test: (password: string) => {
            const oneDigit = /\d/.test(password);
            const oneUpperCase = containsUpperCaseLetters(password);
            return oneDigit && oneUpperCase;
        },
    });

export function getConfirmPasswordValidation(passwordFieldName: string) {
    return Yup.string()
        .ensure()
        .required('Required')
        .oneOf([Yup.ref(passwordFieldName)], 'Passwords must match');
}
