// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
    type MoveCallSuiCommand,
    type SuiArgument,
    type SuiMovePackage,
} from '@mysten/sui.js';
import { type ReactNode } from 'react';

import { flattenSuiArguments } from './utils';

import { ObjectLink } from '~/ui/InternalLink';

export interface CommandProps<T> {
    type: string;
    data: T;
}

function CommandContent({
    type,
    children,
}: {
    type: string;
    children?: ReactNode;
}) {
    return (
        <>
            <div
                data-testid="programmable-transactions-command-label"
                className="text-heading6 font-semibold text-steel-darker"
            >
                {type}
            </div>
            {children && (
                <div
                    data-testid="programmable-transactions-command-content"
                    className="text-bodyMedium pt-2 font-medium text-steel-dark"
                >
                    {children}
                </div>
            )}
        </>
    );
}

function ArrayArgument({
    type,
    data,
}: CommandProps<(SuiArgument | SuiArgument[])[] | undefined>) {
    return (
        <CommandContent type={type}>
            {data && <>({flattenSuiArguments(data)})</>}
        </CommandContent>
    );
}

function MoveCall({ type, data }: CommandProps<MoveCallSuiCommand>) {
    const {
        module,
        package: movePackage,
        function: func,
        arguments: args,
        type_arguments: typeArgs,
    } = data;
    return (
        <CommandContent type={type}>
            (package: <ObjectLink objectId={movePackage} />, module:{' '}
            <ObjectLink
                objectId={`${movePackage}?module=${module}`}
                label={`'${module}'`}
            />
            , function: <span className="text-sui-dark">{func}</span>
            {args && <>, arguments: [{flattenSuiArguments(args!)}]</>}
            {typeArgs && <>, type_arguments: {typeArgs}</>})
        </CommandContent>
    );
}

export function Command({
    type,
    data,
}: CommandProps<
    (SuiArgument | SuiArgument[])[] | MoveCallSuiCommand | SuiMovePackage
>) {
    if (type === 'MoveCall') {
        return <MoveCall type={type} data={data as MoveCallSuiCommand} />;
    }

    return (
        <ArrayArgument
            type={type}
            data={
                type !== 'Publish'
                    ? (data as (SuiArgument | SuiArgument[])[])
                    : undefined
            }
        />
    );
}
