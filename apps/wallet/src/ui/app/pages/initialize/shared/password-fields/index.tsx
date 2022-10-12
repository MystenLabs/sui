// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFormikContext, Field } from 'formik';

import FieldLabel from '_app/shared/field-label';
import PasswordInput from '_app/shared/input/password';
import Alert from '_components/alert';

import st from './PasswordFields.module.scss';

const PASSWORD_INFO_ERROR =
    'Minimum 8 characters. Password must include at least one number and uppercase letter.';

export type PasswordFieldsValues = {
    password: string;
    confirmPassword: string;
};

export default function PasswordFields() {
    const { touched, errors } = useFormikContext<PasswordFieldsValues>();
    return (
        <>
            <FieldLabel txt="Create Password">
                <Field name="password" component={PasswordInput} />
                {touched['password'] && errors['password'] ? (
                    <Alert>{PASSWORD_INFO_ERROR}</Alert>
                ) : (
                    <div className={st.info}>{PASSWORD_INFO_ERROR}</div>
                )}
            </FieldLabel>
            <FieldLabel txt="Confirm Password">
                <Field name="confirmPassword" component={PasswordInput} />
                {touched['confirmPassword'] && errors['confirmPassword'] ? (
                    <Alert>{errors['confirmPassword']}</Alert>
                ) : null}
            </FieldLabel>
        </>
    );
}
