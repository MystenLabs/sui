// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { zodResolver } from '@hookform/resolvers/zod';
import { useForm } from 'react-hook-form';

import type { Resolver } from '@hookform/resolvers/zod';
import type { UseFormProps } from 'react-hook-form';
import type { z, ZodTypeAny } from 'zod';

export function useZodForm<S extends ZodTypeAny, TContext = any>(
    zodSchema: S,
    formOptions?: Omit<UseFormProps<z.infer<S>, TContext>, 'resolver'>,
    zodSchemaOptions?: Parameters<Resolver>['1'],
    zodFactoryOptions?: Parameters<Resolver>['2']
) {
    return useForm<z.infer<S>>({
        ...formOptions,
        resolver: zodResolver(zodSchema, zodSchemaOptions, zodFactoryOptions),
    });
}
