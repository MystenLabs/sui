// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { FieldProps } from 'formik';

import st from './PasswordInput.module.scss';

export type PasswordInputProps = FieldProps;

function PasswordInput({ field, meta, form, ...props }: PasswordInputProps) {
    return <input type="password" {...field} {...props} className={st.input} />;
}

export default PasswordInput;
