// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type MoveCallSuiTransaction,
    type SuiArgument,
    type SuiMovePackage,
} from '@mysten/sui.js';
import { type ReactNode } from 'react';

import { flattenSuiArguments } from './utils';

import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import { ObjectLink } from '~/ui/InternalLink';

export interface TransactionProps<T> {
    type: string;
    data: T;
}

function TransactionContent({
    type,
    children,
}: {
    type: string;
    children?: ReactNode;
}) {
    return (
        <>
            <div className="text-heading6 font-semibold text-steel-darker">
                {type}
            </div>
            {children && (
                <div className="text-bodyMedium pt-2 font-medium text-steel-dark">
                    {children}
                </div>
            )}
        </>
    );
}

function ArrayArgument({
    type,
    data,
}: TransactionProps<(SuiArgument | SuiArgument[])[] | undefined>) {
    return (
        <TransactionContent type={type}>
            {data && <>({flattenSuiArguments(data)})</>}
        </TransactionContent>
    );
}

function MoveCall({ type, data }: TransactionProps<MoveCallSuiTransaction>) {
    const {
        module,
        package: movePackage,
        function: func,
        arguments: args,
        type_arguments: typeArgs,
    } = data;
    return (
        <TransactionContent type={type}>
            (package: <ObjectLink objectId={movePackage} />, module:{' '}
            <ObjectLink
                objectId={`${movePackage}?module=${module}`}
                label={`'${module}'`}
            />
            , function: <span className="text-sui-dark">{func}</span>
            {args && <>, arguments: [{flattenSuiArguments(args!)}]</>}
            {typeArgs && <>, type_arguments: {typeArgs}</>})
        </TransactionContent>
    );
}

export function Transaction({
    type,
    data,
}: TransactionProps<
    (SuiArgument | SuiArgument[])[] | MoveCallSuiTransaction | SuiMovePackage
>) {
    if (type === 'MoveCall') {
        return (
            <ErrorBoundary>
                <MoveCall type={type} data={data as MoveCallSuiTransaction} />
            </ErrorBoundary>
        );
    }

    return (
        <ErrorBoundary>
            <ArrayArgument
                type={type}
                data={
                    type !== 'Publish'
                        ? (data as (SuiArgument | SuiArgument[])[])
                        : undefined
                }
            />
        </ErrorBoundary>
    );
}
