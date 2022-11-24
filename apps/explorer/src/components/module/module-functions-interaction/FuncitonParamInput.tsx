// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { FieldPath, FieldValues, UseFormRegister } from 'react-hook-form';

type FunctionParamInputProps<T extends FieldValues, N extends FieldPath<T>> = {
    paramTypeTxt: string;
    paramIndex: number;
    register: UseFormRegister<T>;
    id: string;
    name: N;
};

export function FunctionParamInput<
    T extends FieldValues,
    N extends FieldPath<T>
>({
    paramTypeTxt,
    paramIndex,
    register,
    id,
    name,
}: FunctionParamInputProps<T, N>) {
    return (
        <div className="flex flex-col flex-nowrap items-stretch gap-2.5">
            <label
                htmlFor={id}
                className="text-bodySmall font-medium text-steel-darker ml-2.5"
            >
                Arg{paramIndex}
            </label>
            <input
                id={id}
                {...register(name)}
                placeholder={paramTypeTxt}
                className="p-2 text-steel-darker text-body font-medium bg-white border-gray-45 border border-solid rounded-md shadow-sm shadow-[#1018280D] placeholder:text-gray-60"
            />
        </div>
    );
}
