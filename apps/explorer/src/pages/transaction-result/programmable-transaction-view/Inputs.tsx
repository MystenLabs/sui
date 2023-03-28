// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiCallArg } from '@mysten/sui.js';

import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { TableHeader } from '~/ui/TableHeader';

interface Props {
    inputs: SuiCallArg[];
}

export function Inputs({ inputs }: Props) {
    if (!inputs?.length) {
        return null;
    }

    return (
        <>
            <TableHeader>Inputs</TableHeader>
            <ul className="flex flex-col gap-y-3">
                {inputs.map((input, index) => {
                    if (typeof input !== 'object') {
                        return (
                            <li key={index}>
                                <AddressLink
                                    noTruncate
                                    address={String(input)}
                                />
                            </li>
                        );
                    }

                    if ('valueType' in input && 'value' in input) {
                        if (input.valueType === 'address') {
                            return (
                                <li key={index}>
                                    <AddressLink
                                        noTruncate
                                        address={String(input.value)}
                                    />
                                </li>
                            );
                        }

                        return (
                            <li key={index}>
                                <div className="mt-1 text-bodySmall font-medium text-steel-dark">
                                    {JSON.stringify(input.value)}
                                </div>
                            </li>
                        );
                    }

                    if (input.type === 'object') {
                        return (
                            <li key={index}>
                                <ObjectLink
                                    noTruncate
                                    objectId={input.objectId}
                                />
                            </li>
                        );
                    }

                    return null;
                })}
            </ul>
        </>
    );
}
