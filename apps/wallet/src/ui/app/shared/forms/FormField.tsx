// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ReactNode } from 'react';
import { useFormContext } from 'react-hook-form';

import Alert from '../../components/alert';
import { FormLabel } from './FormLabel';

type FormFieldProps = {
	name: string;
	label?: ReactNode;
	children: ReactNode;
};

export function FormField({ children, name, label }: FormFieldProps) {
	const { getFieldState, formState } = useFormContext();
	const state = getFieldState(name, formState);

	return (
		<div className="flex flex-col gap-2.5 w-full">
			{label ? <FormLabel label={label}>{children}</FormLabel> : children}
			{state.error && <Alert>{state.error.message}</Alert>}
		</div>
	);
}

export default FormField;
