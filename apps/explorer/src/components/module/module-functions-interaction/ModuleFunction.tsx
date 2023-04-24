// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight12 } from '@mysten/icons';
import {
    getPureSerializationType,
    getExecutionStatusType,
    getExecutionStatusError,
    TransactionBlock,
} from '@mysten/sui.js';
import { useWalletKit, ConnectButton } from '@mysten/wallet-kit';
import { useMutation } from '@tanstack/react-query';
import clsx from 'clsx';
import { useMemo } from 'react';
import { useWatch } from 'react-hook-form';
import { z } from 'zod';

import { FunctionExecutionResult } from './FunctionExecutionResult';
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
    const { isConnected, signAndExecuteTransactionBlock } = useWalletKit();
    const { handleSubmit, formState, register, control } = useZodForm({
        schema: argsSchema,
    });
    const { isValidating, isValid, isSubmitting } = formState;

    const typeArguments = useFunctionTypeArguments(
        functionDetails.typeParameters
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
            const tx = new TransactionBlock();
            tx.moveCall({
                target: `${packageId}::${moduleName}::${functionName}`,
                typeArguments: types ?? [],
                arguments:
                    params?.map((param, i) =>
                        getPureSerializationType(
                            functionDetails.parameters[i],
                            param
                        )
                            ? tx.pure(param)
                            : tx.object(param)
                    ) ?? [],
            });
            const result = await signAndExecuteTransactionBlock({
                transactionBlock: tx,
                options: {
                    showEffects: true,
                    showEvents: true,
                    showInput: true,
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
        isValidating || !isValid || isSubmitting || !isConnected;

    return (
        <DisclosureBox defaultOpen={defaultOpen} title={functionName}>
            <form
                onSubmit={handleSubmit((formData) =>
                    execute.mutateAsync(formData).catch(() => {
                        /* ignore tx execution errors */
                    })
                )}
                autoComplete="off"
                className="flex flex-col flex-nowrap items-stretch gap-4"
            >
                {typeArguments.map((aTypeArgument, index) => (
                    <Input
                        key={index}
                        label={`Type${index}`}
                        {...register(`types.${index}` as const)}
                        placeholder={aTypeArgument}
                    />
                ))}
                {paramsDetails.map(({ paramTypeText }, index) => (
                    <Input
                        key={index}
                        label={`Arg${index}`}
                        {...register(`params.${index}` as const)}
                        placeholder={paramTypeText}
                        disabled={isSubmitting}
                    />
                ))}
                <div className="flex items-stretch justify-end gap-1.5">
                    <Button
                        variant="primary"
                        type="submit"
                        disabled={isExecuteDisabled}
                        loading={execute.isLoading}
                    >
                        Execute
                    </Button>
                    <ConnectButton
                        connectText={
                            <>
                                Connect Wallet
                                <ArrowRight12
                                    fill="currentColor"
                                    className="-rotate-45"
                                />
                            </>
                        }
                        size="md"
                        className={clsx(
                            '!rounded-md !text-bodySmall',
                            isConnected
                                ? '!border !border-solid !border-steel !bg-white !font-mono !text-hero-dark !shadow-sm !shadow-ebony/5'
                                : '!flex !flex-nowrap !items-center !gap-1 !bg-sui-dark !font-sans !text-sui-light hover:!bg-sui-dark hover:!text-white'
                        )}
                    />
                </div>
                {execute.error || execute.data ? (
                    <FunctionExecutionResult
                        error={
                            execute.error
                                ? (execute.error as Error).message || 'Error'
                                : false
                        }
                        result={execute.data || null}
                        onClear={() => {
                            execute.reset();
                        }}
                    />
                ) : null}
            </form>
        </DisclosureBox>
    );
}
