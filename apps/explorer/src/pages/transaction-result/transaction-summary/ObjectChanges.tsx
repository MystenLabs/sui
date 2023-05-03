// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { getGroupByOwner, LocationIdType } from '@mysten/core';
import { ChevronRight12 } from '@mysten/icons';
import {
    type SuiObjectChangeCreated,
    type SuiObjectChangeMutated,
    type SuiObjectChangeTransferred,
} from '@mysten/sui.js';
import clsx from 'clsx';
import { useState } from 'react';

import {
    ExpandableList,
    ExpandableListControl,
    ExpandableListItems,
} from '~/ui/ExpandableList';
import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { Text } from '~/ui/Text';
import {
    TransactionBlockCard,
    TransactionBlockCardSection,
} from '~/ui/TransactionBlockCard';

enum Labels {
    created = 'Created',
    mutated = 'Updated',
    transferred = 'Transfer',
}

enum ItemLabels {
    package = 'Package',
    module = 'Module',
    type = 'Type',
}

type ObjectChangeEntryData<T> = Record<
    string,
    (T & { locationIdType: LocationIdType })[]
>;

const DEFAULT_ITEMS_TO_SHOW = 5;

interface ObjectChangeEntryBaseProps {
    type: keyof typeof Labels;
}

function Item({
    label,
    packageId,
    moduleName,
    typeName,
}: {
    label: ItemLabels;
    packageId: string;
    moduleName: string;
    typeName: string;
}) {
    return (
        <div className="flex justify-between gap-10">
            <Text variant="pBody/medium" color="steel-dark">
                {label}
            </Text>

            {label === ItemLabels.package && (
                <ObjectLink objectId={packageId} />
            )}
            {label === ItemLabels.module && (
                <ObjectLink
                    objectId={`${packageId}?module=${moduleName}`}
                    label={moduleName}
                />
            )}
            {label === ItemLabels.type && (
                <Text truncate variant="pBody/medium" color="steel-darker">
                    {typeName}
                </Text>
            )}
        </div>
    );
}

function ObjectDetail({
    objectType,
    objectId,
}: {
    objectType: string;
    objectId: string;
}) {
    const [expanded, setExpanded] = useState(false);
    const toggleExpand = () => setExpanded((prev) => !prev);

    const [packageId, moduleName, typeName] =
        objectType?.split('<')[0]?.split('::') || [];

    const objectDetailLabels = [
        ItemLabels.package,
        ItemLabels.module,
        ItemLabels.type,
    ];

    return (
        <div>
            <div className="flex justify-between">
                <Link onClick={toggleExpand}>
                    <div className="flex items-center gap-0.5">
                        <Text variant="pBody/medium" color="steel-dark">
                            Object
                        </Text>

                        <ChevronRight12
                            className={clsx(
                                'h-3 w-3 text-steel-dark',
                                expanded && 'rotate-90'
                            )}
                        />
                    </div>
                </Link>

                <ObjectLink objectId={objectId} />
            </div>
            {expanded && (
                <div className="mt-2 flex flex-col gap-2">
                    {objectDetailLabels.map((label) => (
                        <Item
                            key={label}
                            label={label}
                            packageId={packageId}
                            moduleName={moduleName}
                            typeName={typeName}
                        />
                    ))}
                </div>
            )}
        </div>
    );
}

interface ObjectChangeEntriesProps extends ObjectChangeEntryBaseProps {
    changeEntries: (SuiObjectChangeMutated | SuiObjectChangeCreated)[];
}

function ObjectChangeEntries({
    changeEntries,
    type,
}: ObjectChangeEntriesProps) {
    const title = Labels[type];

    return (
        <TransactionBlockCardSection
            title={
                <Text
                    variant="body/semibold"
                    color={
                        title === Labels.created
                            ? 'success-dark'
                            : 'steel-darker'
                    }
                >
                    {title}
                </Text>
            }
        >
            <ExpandableList
                items={changeEntries.map(({ objectId, objectType }) => (
                    <ObjectDetail
                        key={objectId}
                        objectId={objectId}
                        objectType={objectType}
                    />
                ))}
                defaultItemsToShow={DEFAULT_ITEMS_TO_SHOW}
                itemsLabel="Objects"
            >
                <div className="flex max-h-[300px] flex-col gap-3 overflow-y-auto">
                    <ExpandableListItems />
                </div>

                {changeEntries.length > DEFAULT_ITEMS_TO_SHOW && (
                    <div className="mt-4">
                        <ExpandableListControl />
                    </div>
                )}
            </ExpandableList>
        </TransactionBlockCardSection>
    );
}

interface ObjectChangeEntryUpdatedProps extends ObjectChangeEntryBaseProps {
    data:
        | ObjectChangeEntryData<SuiObjectChangeMutated>
        | ObjectChangeEntryData<SuiObjectChangeTransferred>
        | ObjectChangeEntryData<SuiObjectChangeCreated>;
}

export function ObjectChangeEntryUpdated({
    data,
    type,
}: ObjectChangeEntryUpdatedProps) {
    if (!data) {
        return null;
    }

    return (
        <>
            {Object.entries(data).map(([ownerAddress, changes]) => {
                const locationIdType = changes[0].locationIdType;

                const renderFooter =
                    locationIdType === LocationIdType.AddressOwner ||
                    locationIdType === LocationIdType.ObjectOwner ||
                    locationIdType === LocationIdType.Shared;

                return (
                    <TransactionBlockCard
                        key={ownerAddress}
                        title="Changes"
                        size="sm"
                        shadow
                        footer={
                            renderFooter && (
                                <div className="flex items-center justify-between">
                                    <Text
                                        variant="pBody/medium"
                                        color="steel-dark"
                                    >
                                        Owner
                                    </Text>
                                    {locationIdType ===
                                        LocationIdType.AddressOwner && (
                                        <AddressLink address={ownerAddress} />
                                    )}
                                    {locationIdType ===
                                        LocationIdType.ObjectOwner && (
                                        <ObjectLink objectId={ownerAddress} />
                                    )}
                                    {locationIdType ===
                                        LocationIdType.Shared && (
                                        <ObjectLink
                                            objectId={ownerAddress}
                                            label="Shared"
                                        />
                                    )}
                                </div>
                            )
                        }
                    >
                        <ObjectChangeEntries
                            changeEntries={changes}
                            type={type}
                        />
                    </TransactionBlockCard>
                );
            })}
        </>
    );
}

interface ObjectChangesProps {
    objectSummary: {
        mutated: SuiObjectChangeMutated[];
        created: SuiObjectChangeCreated[];
        transferred: SuiObjectChangeTransferred[];
    };
}

export function ObjectChanges({ objectSummary }: ObjectChangesProps) {
    if (!objectSummary) {
        return null;
    }

    const createdChangesByOwner = getGroupByOwner(objectSummary?.created);
    const updatedChangesByOwner = getGroupByOwner(objectSummary?.mutated);
    const transferredChangesByOwner = getGroupByOwner(
        objectSummary?.transferred
    );

    return (
        <>
            {objectSummary?.created?.length ? (
                <ObjectChangeEntryUpdated
                    type="created"
                    data={
                        createdChangesByOwner as unknown as ObjectChangeEntryData<SuiObjectChangeCreated>
                    }
                />
            ) : null}

            {objectSummary.mutated?.length ? (
                <ObjectChangeEntryUpdated
                    type="mutated"
                    data={
                        updatedChangesByOwner as unknown as ObjectChangeEntryData<SuiObjectChangeMutated>
                    }
                />
            ) : null}

            {objectSummary.transferred?.length ? (
                <ObjectChangeEntryUpdated
                    type="transferred"
                    data={
                        transferredChangesByOwner as unknown as ObjectChangeEntryData<SuiObjectChangeTransferred>
                    }
                />
            ) : null}
        </>
    );
}
