// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type WithDisplayFields } from '@mysten/core';
import { ChevronDown12, ChevronRight12 } from '@mysten/icons';
import cx from 'classnames';
import { useState } from 'react';

import {
    type SuiObjectChange,
    type SuiObjectChangeTypes,
} from '../../../../../../../../../sdk/typescript/src';
import { ExpandableList } from '../../../ExpandableList';
import { Card } from '../../Card';
import { OwnerFooter } from '../../OwnerFooter';
import { ObjectDetail } from './ObjectChangeDetail';
import { Text } from '_src/ui/app/shared/text';

interface ObjectChangeEntryProps {
    type: SuiObjectChangeTypes;
    changes: Record<string, WithDisplayFields<SuiObjectChange>[]>;
}

export function ChevronDown({ expanded }: { expanded: boolean }) {
    return expanded ? (
        <ChevronDown12 className="text-gray-45" />
    ) : (
        <ChevronRight12 className="text-gray-45" />
    );
}

const labels = {
    created: 'Created',
    mutated: 'Updated',
    transferred: 'Transferred',
    published: 'Published',
    deleted: 'Deleted',
    wrapped: 'Wrapped',
};

export function ObjectChangeEntry({ changes, type }: ObjectChangeEntryProps) {
    const [expanded, setExpanded] = useState(true);

    return (
        <>
            {Object.entries(changes).map(([ownerKey, changes], index) => {
                return (
                    <Card
                        footer={<OwnerFooter owner={ownerKey} />}
                        key={ownerKey + index}
                        heading="Changes"
                    >
                        <div
                            className={cx(
                                { 'gap-4.5': expanded },
                                'flex flex-col pb-3'
                            )}
                        >
                            <div
                                className="flex w-full flex-col gap-2 cursor-pointer"
                                onClick={() => setExpanded((prev) => !prev)}
                            >
                                <div className="flex w-full items-center gap-2">
                                    <Text
                                        variant="body"
                                        weight="semibold"
                                        color={
                                            type === 'created'
                                                ? 'success-dark'
                                                : 'steel-darker'
                                        }
                                    >
                                        {labels[type]}
                                    </Text>
                                    <div className="h-px bg-gray-40 w-full" />
                                    <ChevronDown expanded={expanded} />
                                </div>
                            </div>
                            <div className="flex w-full flex-col gap-2">
                                <ExpandableList
                                    defaultItemsToShow={5}
                                    items={
                                        expanded && Array.isArray(changes)
                                            ? changes.map((change) => (
                                                  <ObjectDetail
                                                      ownerKey={ownerKey}
                                                      change={change}
                                                  />
                                              ))
                                            : []
                                    }
                                />
                            </div>
                        </div>
                    </Card>
                );
            })}
        </>
    );
}
