// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { forwardRef, type ComponentProps, type ReactNode } from 'react';

import { Input } from './controls/Input';
import { PasswordInput } from './controls/PasswordInput';
import FormField from './FormField';

type TextFieldProps = {
	name: string;
	label?: ReactNode;
} & ComponentProps<'input'>;

export const TextField = forwardRef<HTMLInputElement, TextFieldProps>(
	({ label, ...props }, forwardedRef) => {
		const InputComponent = props.type === 'password' ? PasswordInput : Input;

		return (
			<FormField name={props.name} label={label}>
				<InputComponent {...props} ref={forwardedRef} />
			</FormField>
		);
	},
);
