// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type ComponentProps } from 'react';
import {
	FormProvider,
	type FieldValues,
	type SubmitHandler,
	type UseFormReturn,
} from 'react-hook-form';

type FormProps<T extends FieldValues> = Omit<ComponentProps<'form'>, 'onSubmit'> & {
	form: UseFormReturn<T>;
	onSubmit: SubmitHandler<T>;
};

export function Form<T extends FieldValues>({ form, onSubmit, children, ...props }: FormProps<T>) {
	return (
		<FormProvider {...form}>
			<form onSubmit={form.handleSubmit(onSubmit)} {...props}>
				{children}
			</form>
		</FormProvider>
	);
}
