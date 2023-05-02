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
import { Text } from '~/ui/Text';

export interface TransactionProps<T> {
    type: string;
    data: T;
}

function TransactionContent({ children }: { children?: ReactNode }) {
    return (
        <Text variant="pBody/normal" color="steel-dark">
            {children}
        </Text>
    );
}

function ArrayArgument({
    data,
}: TransactionProps<(SuiArgument | SuiArgument[])[] | undefined>) {
    return (
        <TransactionContent>
            {data && (
                <span className="break-all">({flattenSuiArguments(data)})</span>
            )}
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
        <TransactionContent>
            (package: <ObjectLink objectId={movePackage} />, module:{' '}
            <ObjectLink
                objectId={`${movePackage}?module=${module}`}
                label={`'${module}'`}
            />
            , function: <span className="break-all text-sui-dark">{func}</span>
            {args && (
                <span className="break-all">
                    , arguments: [{flattenSuiArguments(args!)}]
                </span>
            )}
            {typeArgs && (
                <span className="break-all">, type_arguments: {typeArgs}</span>
            )}
            )
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
