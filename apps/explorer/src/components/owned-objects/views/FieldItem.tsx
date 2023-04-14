// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiMoveNormalizedType } from '@mysten/sui.js';

import { SyntaxHighlighter } from '~/components/SyntaxHighlighter';
import { getFieldTypeValue } from '~/components/owned-objects/utils';
import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { Text } from '~/ui/Text';

interface FieldItemProps {
    value: string | number | object | boolean;
    type: SuiMoveNormalizedType;
    objectType: string;
    truncate?: boolean;
}

const TYPE_ADDRESS = 'Address';
const TYPE_URL = '0x2::url::Url';
const TYPE_OBJECT_ID = ['0x2::object::UID', '0x2::object::ID'];

export function FieldItem({
    value,
    type,
    truncate = false,
    objectType,
}: FieldItemProps) {
    // for object types, use SyntaxHighlighter
    if (typeof value === 'object') {
        return (
            <SyntaxHighlighter
                code={JSON.stringify(value, null, 2)}
                language="json"
            />
        );
    }

    const { normalizedType } = getFieldTypeValue(type, objectType);

    if (normalizedType === TYPE_ADDRESS) {
        return (
            <div className="break-all">
                <AddressLink
                    address={value.toString()}
                    noTruncate={!truncate}
                />
            </div>
        );
    }

    if (
        normalizedType === 'string' &&
        TYPE_OBJECT_ID.includes(normalizedType)
    ) {
        return (
            <div className="break-all">
                <ObjectLink
                    objectId={value.toString()}
                    noTruncate={!truncate}
                />
            </div>
        );
    }

    if (normalizedType === TYPE_URL) {
        return (
            <div className="truncate break-all">
                <Link href={value.toString()} variant="textHeroDark">
                    {value}
                </Link>
            </div>
        );
    }

    return (
        <Text variant="body/medium" color="steel-darker" truncate={truncate}>
            {value === null || value === undefined ? null : String(value)}
        </Text>
    );
}
