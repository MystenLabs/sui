// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentProps, type ReactNode, forwardRef } from 'react';
import FormField from './FormField';
import { Input } from './controls/Input';

type NumberField = {
	name: string;
	label?: ReactNode;
} & ComponentProps<'input'>;

export const NumberField = forwardRef<HTMLInputElement, NumberField>(
	({ label, ...props }, forwardedRef) => {
		return (
			<FormField name={props.name} label={label}>
				<Input {...props} ref={forwardedRef} type="number" />
			</FormField>
		);
	},
);
