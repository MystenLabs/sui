// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ChevronRight12 } from '@mysten/icons';
import {
    type SuiObjectChangeCreated,
    type SuiObjectChangeMutated,
    type SuiObjectChangeTransferred,
} from '@mysten/sui.js';
import clsx from 'clsx';
import { useState } from 'react';

import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { Link } from '~/ui/Link';
import { Text } from '~/ui/Text';
import { TransactionCard, TransactionCardSection } from '~/ui/TransactionCard';

enum Labels {
    created = 'Created',
    mutated = 'Updated',
    minted = 'Mint',
    transferred = 'Transfer',
}

enum ItemLabels {
    package = 'Package',
    module = 'Module',
    function = 'Function',
}

enum LocationIdType {
    AddressOwner = 'AddressOwner',
    ObjectOwner = 'ObjectOwner',
    Shared = 'Shared',
    Unknown = 'Unknown',
}

interface ObjectChangeEntryBaseProps {
    type: keyof typeof Labels;
}

function Item({
    label,
    packageId,
    moduleName,
    functionName,
}: {
    label: ItemLabels;
    packageId: string;
    moduleName: string;
    functionName: string;
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
            {label === ItemLabels.function && (
                <Text truncate variant="pBody/medium" color="steel-darker">
                    {functionName}
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

    const regex = /^(.*?)::(.*?)::(.*)$/;
    const [, packageId, moduleName, functionName] =
        objectType.match(regex) ?? [];

    const objectDetailLabels = [
        ItemLabels.package,
        ItemLabels.module,
        ItemLabels.function,
    ];

    return (
        <>
            <div className="flex justify-between">
                <Link
                    gap="xs"
                    variant="text"
                    onClick={toggleExpand}
                    after={
                        <ChevronRight12
                            height={12}
                            width={12}
                            className={clsx(
                                'text-steel-dark',
                                expanded && 'rotate-90'
                            )}
                        />
                    }
                >
                    <Text variant="pBody/medium" color="steel-dark">
                        Object
                    </Text>
                </Link>

                <ObjectLink objectId={objectId} />
                {/* {minted && <NFTDetails objectId={objectId} />} */}
            </div>
            {expanded && (
                <div className="flex flex-col gap-1">
                    {objectDetailLabels.map((label) => (
                        <Item
                            key={label}
                            label={label}
                            packageId={packageId}
                            moduleName={moduleName}
                            functionName={functionName}
                        />
                    ))}
                </div>
            )}
        </>
    );
}

interface ObjectChangeEntryProps extends ObjectChangeEntryBaseProps {
    changeEntries: (
        | (SuiObjectChangeMutated & { minted: boolean })
        | (SuiObjectChangeCreated & { minted: boolean })
    )[];
}

function ObjectChangeEntry({ changeEntries, type }: ObjectChangeEntryProps) {
    const title = Labels[type];

    return (
        <TransactionCardSection
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
            {changeEntries?.map(({ objectType, objectId }) => (
                <ObjectDetail
                    key={objectId}
                    objectType={objectType}
                    objectId={objectId}
                />
            ))}
        </TransactionCardSection>
    );
}

interface ObjectChangeEntryUpdatedProps extends ObjectChangeEntryBaseProps {
    data: Record<
        string,
        SuiObjectChangeMutated[] &
            { minted: boolean; locationIdType: LocationIdType }[]
    >;
}

export function ObjectChangeEntryUpdated({
    data,
    type,
}: ObjectChangeEntryUpdatedProps) {
    if (!data) {
        return null;
    }

    const title = Labels[type];

    const changeObjectEntries = Object.entries(data);

    return (
        <>
            {changeObjectEntries.map(([ownerAddress, changes]) => {
                const locationIdType = changes[0].locationIdType;

                const renderFooter =
                    locationIdType === LocationIdType.AddressOwner ||
                    locationIdType === LocationIdType.ObjectOwner ||
                    locationIdType === LocationIdType.Shared;

                return (
                    <TransactionCard
                        key={ownerAddress}
                        title="Changes"
                        size="sm"
                        shadow="default"
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
                        <TransactionCardSection
                            title={
                                <Text
                                    variant="body/semibold"
                                    color="steel-darker"
                                >
                                    {title}
                                </Text>
                            }
                        >
                            {changes.map(({ objectId, objectType }) => (
                                <ObjectDetail
                                    key={objectId}
                                    objectId={objectId}
                                    objectType={objectType}
                                />
                            ))}
                        </TransactionCardSection>
                    </TransactionCard>
                );
            })}
        </>
    );
}

function groupByOwner(
    objectSummaryChanges: (
        | SuiObjectChangeMutated
        | SuiObjectChangeCreated
        | SuiObjectChangeTransferred
    )[]
) {
    if (!objectSummaryChanges) {
        return {};
    }

    return objectSummaryChanges?.reduce(
        (mapByOwner: Record<string, any[]>, change: any) => {
            const owner = change?.owner;

            let key = '';
            let locationIdType;
            if ('AddressOwner' in owner) {
                key = owner.AddressOwner;
                locationIdType = LocationIdType.AddressOwner;
            } else if ('ObjectOwner' in owner) {
                key = owner.ObjectOwner;
                locationIdType = LocationIdType.ObjectOwner;
            } else if ('Shared' in owner) {
                key = change.objectId;
                locationIdType = LocationIdType.Shared;
            } else {
                const ownerKeys = Object.keys(owner);
                const firstKey = ownerKeys[0];
                key = owner[firstKey];
                locationIdType = LocationIdType.Unknown;
            }

            mapByOwner[key] = mapByOwner[key] || [];
            mapByOwner[key].push({
                ...change,
                locationIdType,
            });

            return mapByOwner;
        },
        {}
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

    const createdChangesByOwner = groupByOwner(objectSummary?.created);
    const createdChangesData = Object.values(createdChangesByOwner);

    const updatedChangesByOwner = groupByOwner(objectSummary?.mutated);

    const transferredChangesByOwner = groupByOwner(objectSummary?.transferred);

    return (
        <>
            {objectSummary?.created?.length ? (
                <TransactionCard title="Changes" size="sm" shadow="default">
                    {createdChangesData.map((data, index) => (
                        <ObjectChangeEntry
                            key={index}
                            type="created"
                            changeEntries={data}
                        />
                    ))}
                </TransactionCard>
            ) : null}

            {objectSummary.mutated?.length ? (
                <ObjectChangeEntryUpdated
                    type="mutated"
                    data={updatedChangesByOwner}
                />
            ) : null}

            {objectSummary.transferred?.length ? (
                <ObjectChangeEntryUpdated
                    type="transferred"
                    data={transferredChangesByOwner}
                />
            ) : null}
        </>
    );
}
