// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {type SuiMoveNormalizedType } from '@mysten/sui.js';

import { SyntaxHighlighter } from '~/components/SyntaxHighlighter';
import { extractSerializationType, getFieldTypeValue, FieldTypeValue } from '~/components/ownedobjects/utils';
import { AddressLink, ObjectLink, TransactionLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { Text } from '~/ui/Text';


interface FieldItemProps<T> {
    value: T;
    type: SuiMoveNormalizedType;
    truncate?: boolean;
}

export function FieldItem<T>({ value, type, truncate = false }: FieldItemProps<T>) {
    
    // for object types, use SyntaxHighlighter
    if (typeof value === 'object') {
        return (
            <SyntaxHighlighter
                code={JSON.stringify(value, null, 2)}
                language="json"
            />
        );
    }
    const normalizedType = extractSerializationType(type);

    if (typeof value === 'string' && normalizedType === 'Address') {
        return (
            <div className="break-all">
                <AddressLink address={value} noTruncate={!truncate}/>
            </div>
        );
    }

    if (typeof value === 'string' && getFieldTypeValue(normalizedType, FieldTypeValue.ADDRESS) === 'Address') {
        return (
            <div className="break-all">
                <ObjectLink objectId={value} noTruncate={!truncate}/>
            </div>
        );
    }

    // TODO: verify this is correct
    if (typeof value === 'string' && type === 'digest') {
        return (
            <div className="break-all">
                <TransactionLink digest={value} />
            </div>
        );
    }

    if(typeof value === 'string' &&  getFieldTypeValue(normalizedType, FieldTypeValue.MODULE) === 'url') {  
        return (
            <div className="break-all truncate">
                <Link href={value} variant="textHeroDark">
                    {value}
                </Link>
            </div>
        )
        
    }

    return (
        <Text variant="body/medium" color="steel-darker">
            {value?.toString()}
        </Text>
    );
}
