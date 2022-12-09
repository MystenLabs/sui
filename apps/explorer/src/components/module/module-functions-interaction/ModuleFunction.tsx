// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getExecutionStatusError,
} from '@mysten/sui.js';
import { useWallet, ConnectButton } from '@mysten/wallet-kit';
import { useMutation } from '@tanstack/react-query';
import clsx from 'clsx';
import { useMemo } from 'react';
import { useWatch } from 'react-hook-form';
import toast from 'react-hot-toast';
import { z } from 'zod';

import { ReactComponent as ArrowRight } from '../../../assets/SVGIcons/12px/ArrowRight.svg';
import { useFunctionParamsDetails } from './useFunctionParamsDetails';
import { useFunctionTypeArguments } from './useFunctionTypeArguments';

import type { SuiMoveNormalizedFunction, ObjectId } from '@mysten/sui.js';
import type { TypeOf } from 'zod';

import { useZodForm } from '~/hooks/useZodForm';
import { Button } from '~/ui/Button';
import { DisclosureBox } from '~/ui/DisclosureBox';
import { Input } from '~/ui/Input';

const argsSchema = z.object({
    params: z.optional(z.array(z.string().trim().min(1))),
    types: z.optional(z.array(z.string().trim().min(1))),
});

export type ModuleFunctionProps = {
    packageId: ObjectId;
    moduleName: string;
    functionName: string;
    functionDetails: SuiMoveNormalizedFunction;
    defaultOpen?: boolean;
};

export function ModuleFunction({
    defaultOpen,
    packageId,
    moduleName,
    functionName,
    functionDetails,
}: ModuleFunctionProps) {
    const { connected, signAndExecuteTransaction } = useWallet();
    const { handleSubmit, formState, register, control } = useZodForm({
        schema: argsSchema,
    });
    const typeArguments = useFunctionTypeArguments(
        functionDetails.type_parameters
    );
    const formTypeInputs = useWatch({ control, name: 'types' });
    const resolvedTypeArguments = useMemo(
        () =>
            typeArguments.map(
                (aType, index) => formTypeInputs?.[index] || aType
            ),
        [typeArguments, formTypeInputs]
    );
    const paramsDetails = useFunctionParamsDetails(
        functionDetails.parameters,
        resolvedTypeArguments
    );
    const execute = useMutation({
        mutationFn: async ({ params, types }: TypeOf<typeof argsSchema>) => {
            const result = await signAndExecuteTransaction({
                kind: 'moveCall',
                data: {
                    packageObjectId: packageId,
                    module: moduleName,
                    function: functionName,
                    arguments: params || [],
                    typeArguments: types || [],
                    gasBudget: 2000,
                },
            });
            if (getExecutionStatusType(result) === 'failure') {
                throw new Error(
                    getExecutionStatusError(result) || 'Transaction failed'
                );
            }
            return result;
        },
    });
    const isExecuteDisabled =
        formState.isValidating ||
        !formState.isValid ||
        formState.isSubmitting ||
        !connected;
    return (
        <DisclosureBox defaultOpen={defaultOpen} title={functionName}>
            <form
                onSubmit={handleSubmit((formData) =>
                    toast
                        .promise(execute.mutateAsync(formData), {
                            loading: 'Executing...',
                            error: (e) => 'Transaction failed',
                            success: 'Done',
                        })
                        .catch((e) => null)
                )}
                autoComplete="off"
                className="flex flex-col flex-nowrap items-stretch gap-4"
            >
                {typeArguments.map((aTypeArgument, index) => {
                    return (
                        <Input
                            key={index}
                            label={`Type${index}`}
                            {...register(`types.${index}` as const)}
                            placeholder={aTypeArgument}
                        />
                    );
                })}
                {paramsDetails.map(({ paramTypeText }, index) => {
                    return (
                        <Input
                            key={index}
                            label={`Arg${index}`}
                            {...register(`params.${index}` as const)}
                            placeholder={paramTypeText}
                        />
                    );
                })}
                <div className="flex items-stretch justify-end gap-1.5">
                    <Button
                        variant="primary"
                        type="submit"
                        disabled={isExecuteDisabled}
                    >
                        Execute
                    </Button>
                    <ConnectButton
                        connectText={
                            <>
                                Connect Wallet
                                <ArrowRight
                                    fill="currentColor"
                                    className="-rotate-45"
                                />
                            </>
                        }
                        size="md"
                        className={clsx(
                            '!rounded-md !text-bodySmall',
                            connected
                                ? '!border !border-solid !border-steel !font-mono !text-hero-dark !shadow-sm !shadow-ebony/5'
                                : '!flex !flex-nowrap !items-center !gap-1 !bg-sui-dark !font-sans !text-sui-light hover:!bg-sui-dark hover:!text-white'
                        )}
                    />
                </div>
            </form>
        </DisclosureBox>
    );
}
