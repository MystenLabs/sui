// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormikContext } from 'formik';

import FieldLabel from '_app/shared/field-label';
import { PasswordInputField } from '_app/shared/input/password';
import Alert from '_components/alert';

export type PasswordFieldsValues = {
	password: string;
	confirmPassword: string;
};

export default function PasswordFields() {
	const { touched, errors } = useFormikContext<PasswordFieldsValues>();
	return (
		<>
			<FieldLabel txt="Create Password">
				<PasswordInputField name="password" />
				{touched.password && errors.password ? <Alert>{errors.password}</Alert> : null}
			</FieldLabel>
			<FieldLabel txt="Confirm Password">
				<PasswordInputField name="confirmPassword" />
				{touched.confirmPassword && errors.confirmPassword ? (
					<Alert>{errors.confirmPassword}</Alert>
				) : null}
			</FieldLabel>
		</>
	);
}
