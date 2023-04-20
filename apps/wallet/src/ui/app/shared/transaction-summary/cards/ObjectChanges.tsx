// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type ObjectChangeSummary } from '@mysten/core';
import { ChevronDown12, ChevronRight12 } from '@mysten/icons';
import {
    formatAddress,
    type SuiObjectChangeCreated,
    type SuiObjectChangeMutated,
} from '@mysten/sui.js';
import cx from 'classnames';
import { useState } from 'react';

import { ExpandableList } from '../../ExpandableList';
import { Card } from '../Card';
import ExplorerLink from '_src/ui/app/components/explorer-link';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';
import { Text } from '_src/ui/app/shared/text';

const labels = {
    created: 'Create',
    mutated: 'Update',
};

interface ObjectChangeEntryProps {
    type: 'created' | 'mutated';
    changes: SuiObjectChangeCreated[] | SuiObjectChangeMutated[];
}

function ChevronDown({ expanded }: { expanded: boolean }) {
    return expanded ? (
        <ChevronDown12 className="text-gray-45" />
    ) : (
        <ChevronRight12 className="text-gray-45" />
    );
}

function ObjectDetail({
    objectId,
    objectType,
}: {
    objectId: string;
    objectType: string;
}) {
    const [expanded, setExpanded] = useState(false);

    const [packageId, moduleName, functionName] =
        objectType?.split('<')[0]?.split('::') || [];

    return (
        <div className="flex flex-col gap-1">
            <div className="grid grid-cols-2 overflow-auto cursor-pointer">
                <div
                    className="flex items-center gap-1 text-steel-dark hover:text-steel-darker select-none"
                    onClick={() => setExpanded((prev) => !prev)}
                >
                    <Text variant="pBodySmall">Object</Text>
                    <ChevronDown expanded={expanded} />
                </div>

                <div className="justify-self-end">
                    <ExplorerLink
                        type={ExplorerLinkType.object}
                        objectID={objectId}
                        className="text-hero-dark no-underline"
                    >
                        <Text variant="pBodySmall" truncate mono>
                            {formatAddress(objectId)}
                        </Text>
                    </ExplorerLink>
                </div>
            </div>
            {expanded && (
                <div className="flex flex-col gap-1">
                    <div className="grid grid-cols-2 overflow-auto relative">
                        <Text variant="pBodySmall" color="steel-dark">
                            Package
                        </Text>

                        <div className="flex justify-end">
                            <ExplorerLink
                                type={ExplorerLinkType.object}
                                objectID={packageId}
                                className="text-hero-dark no-underline justify-self-end overflow-auto"
                            >
                                <Text variant="pBodySmall" truncate mono>
                                    {packageId}
                                </Text>
                            </ExplorerLink>
                        </div>
                    </div>
                    <div className="grid grid-cols-2 overflow-auto">
                        <Text variant="pBodySmall" color="steel-dark">
                            Module
                        </Text>

                        <div className="flex justify-end">
                            <ExplorerLink
                                type={ExplorerLinkType.object}
                                objectID={packageId}
                                moduleName={moduleName}
                                className="text-hero-dark no-underline justify-self-end overflow-auto"
                            >
                                <Text variant="pBodySmall" truncate mono>
                                    {moduleName}
                                </Text>
                            </ExplorerLink>
                        </div>
                    </div>
                    <div className="grid grid-cols-2 overflow-auto">
                        <Text variant="pBodySmall" color="steel-dark">
                            Function
                        </Text>

                        <div className="flex justify-end">
                            <ExplorerLink
                                type={ExplorerLinkType.object}
                                objectID={packageId}
                                moduleName={moduleName}
                                className="text-hero-dark no-underline justify-self-end overflow-auto"
                            >
                                <Text variant="pBodySmall" truncate mono>
                                    {functionName}
                                </Text>
                            </ExplorerLink>
                        </div>
                    </div>
                </div>
            )}
        </div>
    );
}

function ObjectChangeEntry({ changes, type }: ObjectChangeEntryProps) {
    const [expanded, setExpanded] = useState(true);
    if (!changes.length) return null;

    return (
        <Card heading="Changes">
            <div className={cx({ 'gap-4.5': expanded }, 'flex flex-col')}>
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
                        <div className="h-[1px] bg-gray-40 w-full" />
                        <ChevronDown expanded={expanded} />
                    </div>
                </div>
                <div className="flex w-full flex-col gap-2">
                    <ExpandableList
                        defaultItemsToShow={5}
                        items={
                            expanded
                                ? changes?.map(({ objectType, objectId }) => (
                                      <ObjectDetail
                                          objectId={objectId}
                                          objectType={objectType}
                                      />
                                  ))
                                : []
                        }
                    />
                </div>
            </div>
        </Card>
    );
}

export function ObjectChanges({
    changes,
}: {
    changes?: ObjectChangeSummary | null;
}) {
    if (!changes) return null;
    return (
        <>
            <ObjectChangeEntry type="mutated" changes={changes.mutated} />
            <ObjectChangeEntry type="created" changes={changes.created} />
        </>
    );
}
