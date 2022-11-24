// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useWallet } from '@mysten/wallet-adapter-react';
import { WalletWrapper } from '@mysten/wallet-adapter-react-ui';
import clsx from 'clsx';
import { useMemo } from 'react';
import { useFieldArray } from 'react-hook-form';
import toast from 'react-hot-toast';
import { z } from 'zod';

import { FunctionParamInput } from './FuncitonParamInput';
import { useFunctionParamsDetails } from './useFunctionParamsDetails';

import type { SuiMoveNormalizedFunction, ObjectId } from '@mysten/sui.js';

import { useZodForm } from '~/hooks/useZodForm';
import { Button } from '~/ui/Button';
import { DisclosureBox } from '~/ui/DisclosureBox';

const argsSchema = z.object({
    params: z.array(z.object({ value: z.string().trim().min(1) })),
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
    const allParamsDetails = useFunctionParamsDetails(
        functionDetails.parameters
    );
    const filteredParamsDetails = useMemo(
        () => allParamsDetails.filter((aParam) => !aParam.isTxContext),
        [allParamsDetails]
    );
    const defaultValues = useMemo(
        () =>
            Array.from({ length: filteredParamsDetails.length }, () => ({
                value: '',
            })),
        [filteredParamsDetails.length]
    );
    const { register, handleSubmit, formState, control } = useZodForm(
        argsSchema,
        {
            defaultValues: {
                params: defaultValues,
            },
        }
    );
    const { fields } = useFieldArray({ control, name: 'params' });
    const isExecuteDisabled =
        formState.isValidating ||
        !formState.isValid ||
        formState.isSubmitting ||
        !connected;
    return (
        <DisclosureBox defaultOpen={defaultOpen} title={functionName}>
            <form
                onSubmit={handleSubmit(async ({ params }) => {
                    try {
                        await toast.promise(
                            signAndExecuteTransaction({
                                kind: 'moveCall',
                                data: {
                                    packageObjectId: packageId,
                                    module: moduleName,
                                    function: functionName,
                                    arguments: params.map(({ value }) => value),
                                    typeArguments: [], // TODO: currently move calls that expect type argument will fail
                                    gasBudget: 2000,
                                },
                            }).then((tx) => {
                                if (tx.effects.status.status === 'failure') {
                                    throw new Error(
                                        tx.effects.status.error ||
                                            'Transaction failed'
                                    );
                                }
                            }),
                            {
                                loading: 'Executing...',
                                error: (e) => 'Transaction failed',
                                success: 'Done',
                            }
                        );
                    } catch (e) {}
                })}
                autoComplete="off"
                className="flex flex-col flex-nowrap items-stretch gap-3.75"
            >
                {filteredParamsDetails.map(({ paramTypeTxt }, index) => (
                    <FunctionParamInput
                        key={fields[index].id}
                        id={fields[index].id}
                        paramTypeTxt={paramTypeTxt}
                        paramIndex={index}
                        register={register}
                        name={`params.${index}.value`}
                    />
                ))}
                <div className="flex items-center justify-end gap-1.5">
                    <Button
                        variant="primary"
                        type="submit"
                        disabled={isExecuteDisabled}
                    >
                        Execute
                    </Button>
                    <div className={clsx('temp-ui-override', { connected })}>
                        <WalletWrapper />
                    </div>
                </div>
            </form>
        </DisclosureBox>
    );
}
