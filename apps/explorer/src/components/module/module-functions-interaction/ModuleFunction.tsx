// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    getExecutionStatusType,
    getExecutionStatusError,
} from '@mysten/sui.js';
import { useWallet, ConnectButton } from '@mysten/wallet-kit';
import { useMutation } from '@tanstack/react-query';
import clsx from 'clsx';
import { useEffect } from 'react';
import { useWatch } from 'react-hook-form';
import { z } from 'zod';

import { ReactComponent as ArrowRight } from '../../../assets/SVGIcons/12px/ArrowRight.svg';
import { FunctionExecutionResult } from './FunctionExecutionResult';
import { useFunctionParamsDetails } from './useFunctionParamsDetails';

import type { SuiMoveNormalizedFunction, ObjectId } from '@mysten/sui.js';

import { useZodForm } from '~/hooks/useZodForm';
import { Button } from '~/ui/Button';
import { DisclosureBox } from '~/ui/DisclosureBox';
import { Input } from '~/ui/Input';
import { LoadingSpinner } from '~/ui/LoadingSpinner';

const argsSchema = z.object({
    params: z.optional(z.array(z.object({ value: z.string().trim().min(1) }))),
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
    const paramsDetails = useFunctionParamsDetails(functionDetails.parameters);
    const { handleSubmit, formState, register, control, reset } = useZodForm({
        schema: argsSchema,
    });
    const execute = useMutation({
        mutationFn: async (params: string[]) => {
            const result = await signAndExecuteTransaction({
                kind: 'moveCall',
                data: {
                    packageObjectId: packageId,
                    module: moduleName,
                    function: functionName,
                    arguments: params,
                    typeArguments: [], // TODO: currently move calls that expect type argument will fail
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
    const executeFunctionError = execute.error
        ? (execute.error as Error).message
        : false;
    const allValues = useWatch({ control });
    const mutationReset = execute.reset;
    useEffect(() => {
        mutationReset();
    }, [allValues, mutationReset]);
    const isExecuteDisabled =
        formState.isValidating ||
        !formState.isValid ||
        formState.isSubmitting ||
        !connected;
    return (
        <DisclosureBox defaultOpen={defaultOpen} title={functionName}>
            <form
                onSubmit={handleSubmit(({ params }) =>
                    execute
                        .mutateAsync((params || []).map(({ value }) => value))
                        .catch((e) => null)
                )}
                autoComplete="off"
                className="flex flex-col flex-nowrap items-stretch gap-4"
            >
                {paramsDetails.map(({ paramTypeText }, index) => (
                    <Input
                        key={index}
                        label={`Arg${index}`}
                        {...register(`params.${index}.value` as const)}
                        placeholder={paramTypeText}
                    />
                ))}
                <div className="flex items-center justify-end gap-1.5">
                    <Button
                        variant="primary"
                        type="submit"
                        disabled={isExecuteDisabled}
                    >
                        {execute.isLoading ? <LoadingSpinner /> : 'Execute'}
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
                {executeFunctionError || execute.data ? (
                    <FunctionExecutionResult
                        error={executeFunctionError}
                        result={execute.data || null}
                        onClear={() => {
                            execute.reset();
                            reset();
                        }}
                    />
                ) : null}
            </form>
        </DisclosureBox>
    );
}
