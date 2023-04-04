// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SyntaxHighlightedCode } from './SyntaxHighlightedCode';

import { AddressLink, ObjectLink, TransactionLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';

interface FieldItemProps<T> {
    value: T;
    type?: string;
}

export function FieldItem<T>({ value, type }: FieldItemProps<T>) {
    if (typeof value === 'object') {
        return (
            <SyntaxHighlightedCode
                code={JSON.stringify(value, null, 2)}
                language="json"
            />
        );
    }
    if (typeof value === 'string' && type === 'address') {
        return <AddressLink address={value} />;
    }

    if (typeof value === 'string' && type === 'objectId') {
        return (
            <div className="break-all">
                <ObjectLink objectId={value} />
            </div>
        );
    }

    if (typeof value === 'string' && type === 'digest') {
        return <TransactionLink digest={value} />;
    }

    return (
        <Text variant="body/medium" color="steel-darker">
            {value?.toString()}
        </Text>
    );
}
