// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { zodResolver } from '@hookform/resolvers/zod';
import { useForm } from 'react-hook-form';

import type { UseFormProps } from 'react-hook-form';
import type { ZodSchema, TypeOf } from 'zod';

interface UseZodFormProps<T extends ZodSchema<any>> extends UseFormProps<TypeOf<T>> {
	schema: T;
}

export const useZodForm = <T extends ZodSchema<any>>({
	schema,
	...formConfig
}: UseZodFormProps<T>) =>
	useForm({
		...formConfig,
		resolver: zodResolver(schema),
	});
