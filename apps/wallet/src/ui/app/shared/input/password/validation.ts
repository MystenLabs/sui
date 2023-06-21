// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import * as Yup from 'yup';
import zxcvbn from 'zxcvbn';

function addDot(str: string | undefined) {
	if (str && !str.endsWith('.')) {
		return `${str}.`;
	}
	return str;
}

export const passwordValidation = Yup.string()
	.ensure()
	.required('Required')
	.test({
		name: 'password-strength',
		test: (password: string) => {
			return zxcvbn(password).score > 2;
		},
		message: ({ value }) => {
			const {
				feedback: { warning, suggestions },
			} = zxcvbn(value);
			return `${addDot(warning) || 'Password is not strong enough.'}${
				suggestions ? ` ${suggestions.join(' ')}` : ''
			}`;
		},
	});

export function getConfirmPasswordValidation(passwordFieldName: string) {
	return Yup.string()
		.ensure()
		.required('Required')
		.oneOf([Yup.ref(passwordFieldName)], 'Passwords must match');
}
