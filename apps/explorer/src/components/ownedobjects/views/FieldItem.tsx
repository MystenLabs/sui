// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiMoveNormalizedType } from '@mysten/sui.js';

import { SyntaxHighlighter } from '~/components/SyntaxHighlighter';
import {
    extractSerializationType,
    getFieldTypeValue,
    FieldValueType,
} from '~/components/ownedobjects/utils';
import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { Text } from '~/ui/Text';

interface FieldItemProps<T> {
    value: T;
    type: SuiMoveNormalizedType;
    truncate?: boolean;
}

export function FieldItem<T>({
    value,
    type,
    truncate = false,
}: FieldItemProps<T>) {
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
    const moduleName = getFieldTypeValue(normalizedType, FieldValueType.MODULE);
    const address = getFieldTypeValue(normalizedType, FieldValueType.ADDRESS);
    const name = getFieldTypeValue(normalizedType, FieldValueType.NAME);

    if (typeof value === 'string' && normalizedType === 'Address') {
        return (
            <div className="break-all">
                <AddressLink address={value} noTruncate={!truncate} />
            </div>
        );
    }

    const isObjectId = ['0x2::object::UID', '0x2::object::ID'].includes(
        `${address}::${moduleName}::${name}`
    );
    if (typeof value === 'string' && isObjectId) {
        return (
            <div className="break-all">
                <ObjectLink objectId={value} noTruncate={!truncate} />
            </div>
        );
    }

    if (typeof value === 'string' && moduleName === 'url') {
        return (
            <div className="truncate break-all">
                <Link href={value} variant="textHeroDark">
                    {value}
                </Link>
            </div>
        );
    }

    return (
        <Text variant="body/medium" color="steel-darker">
            {value?.toString()}
        </Text>
    );
}
