// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';
import {
    getOwnerType,
    type ObjectChangeSummary,
    type SuiObjectChangeWithDisplay,
} from '@mysten/core';
import { ChevronRight12 } from '@mysten/icons';
import {
    type SuiObjectChangeTypes,
    type SuiObjectChangeCreated,
    type SuiObjectChangeMutated,
    type SuiObjectChangePublished,
} from '@mysten/sui.js';
import clsx from 'clsx';
import { type ReactNode } from 'react';

import {
    ExpandableList,
    ExpandableListControl,
    ExpandableListItems,
} from '~/ui/ExpandableList';
import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { Text } from '~/ui/Text';
import {
    TransactionBlockCard,
    TransactionBlockCardSection,
} from '~/ui/TransactionBlockCard';

enum Labels {
    created = 'Created',
    mutated = 'Updated',
    transferred = 'Transfer',
    published = 'Publish',
    wrapped = 'Wrap',
    deleted = 'Delete',
}

enum ItemLabels {
    package = 'Package',
    module = 'Module',
    type = 'Type',
}

const DEFAULT_ITEMS_TO_SHOW = 5;

function Item({
    label,
    packageId,
    moduleName,
    typeName,
}: {
    label: ItemLabels;
    packageId?: string;
    moduleName?: string;
    typeName?: string;
}) {
    return (
        <div
            className={clsx(
                'flex justify-between gap-10',
                label === ItemLabels.type ? 'items-start' : 'items-center'
            )}
        >
            <Text variant="pBody/medium" color="steel-dark">
                {label}
            </Text>

            {label === ItemLabels.package && packageId && (
                <ObjectLink objectId={packageId} />
            )}
            {label === ItemLabels.module && (
                <ObjectLink
                    objectId={`${packageId}?module=${moduleName}`}
                    label={moduleName}
                />
            )}
            {label === ItemLabels.type && (
                <div className="break-all text-right">
                    <Text variant="pBody/medium" color="steel-darker">
                        {typeName}
                    </Text>
                </div>
            )}
        </div>
    );
}

function ObjectDetailPanel({
    panelContent,
    objectId,
}: {
    panelContent: ReactNode;
    objectId?: string;
}) {
    return (
        <div>
            <Disclosure>
                {({ open }) => (
                    <>
                        <div className="flex flex-wrap items-center justify-between">
                            <Disclosure.Button>
                                <div className="flex items-center gap-0.5">
                                    <Text
                                        variant="pBody/medium"
                                        color="steel-dark"
                                    >
                                        Object
                                    </Text>

                                    <ChevronRight12
                                        className={clsx(
                                            'h-3 w-3 text-steel-dark',
                                            open && 'rotate-90'
                                        )}
                                    />
                                </div>
                            </Disclosure.Button>

                            {objectId && <ObjectLink objectId={objectId} />}
                        </div>

                        <Disclosure.Panel>
                            <div className="flex flex-col gap-2">
                                {panelContent}
                            </div>
                        </Disclosure.Panel>
                    </>
                )}
            </Disclosure>
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
    const separator = '::';
    const objectTypeSplit = objectType?.split(separator) || [];
    const packageId = objectTypeSplit[0];
    const moduleName = objectTypeSplit[1];
    const typeName = objectTypeSplit.slice(2).join(separator);

    const objectDetailLabels = [
        ItemLabels.package,
        ItemLabels.module,
        ItemLabels.type,
    ];

    return (
        <ObjectDetailPanel
            objectId={objectId}
            panelContent={
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
            }
        />
    );
}

interface ObjectChangeEntriesProps {
    type: SuiObjectChangeTypes;
    changeEntries: SuiObjectChangeWithDisplay[];
}

function ObjectChangeEntries({
    changeEntries,
    type,
}: ObjectChangeEntriesProps) {
    const title = Labels[type];

    let expandableItems = [];

    if (type === 'published') {
        expandableItems = (changeEntries as SuiObjectChangePublished[]).map(
            ({ packageId, modules }) => (
                <ObjectDetailPanel
                    key={packageId}
                    panelContent={
                        <div className="mt-2 flex flex-col gap-2">
                            <Item
                                label={ItemLabels.package}
                                packageId={packageId}
                            />
                            {modules.map((moduleName, index) => (
                                <Item
                                    key={index}
                                    label={ItemLabels.module}
                                    moduleName={moduleName}
                                    packageId={packageId}
                                />
                            ))}
                        </div>
                    }
                />
            )
        );
    } else {
        expandableItems = (
            changeEntries as (SuiObjectChangeMutated | SuiObjectChangeCreated)[]
        ).map(({ objectId, objectType }) => (
            <ObjectDetail
                key={objectId}
                objectId={objectId}
                objectType={objectType}
            />
        ));
    }

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
                items={expandableItems}
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

interface ObjectChangeEntriesCardsProps {
    data: Record<string, SuiObjectChangeWithDisplay[]>;
    type: SuiObjectChangeTypes;
}

export function ObjectChangeEntriesCards({
    data,
    type,
}: ObjectChangeEntriesCardsProps) {
    if (!data) return null;

    return (
        <>
            {Object.entries(data).map(([ownerAddress, changes]) => {
                const ownerType = getOwnerType(changes[0]);
                const renderFooter = [
                    'AddressOwner',
                    'ObjectOwner',
                    'Shared',
                ].includes(ownerType);
                return (
                    <TransactionBlockCard
                        key={ownerAddress}
                        title="Changes"
                        size="sm"
                        shadow
                        footer={
                            renderFooter && (
                                <div className="flex flex-wrap items-center justify-between">
                                    <Text
                                        variant="pBody/medium"
                                        color="steel-dark"
                                    >
                                        Owner
                                    </Text>

                                    {ownerType === 'AddressOwner' && (
                                        <AddressLink address={ownerAddress} />
                                    )}

                                    {ownerType === 'ObjectOwner' && (
                                        <ObjectLink objectId={ownerAddress} />
                                    )}

                                    {ownerType === 'Shared' && (
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
    objectSummary: ObjectChangeSummary;
}

export function ObjectChanges({ objectSummary }: ObjectChangesProps) {
    if (!objectSummary) return null;

    return (
        <>
            {Object.entries(objectSummary).map(([type, changes]) => (
                <ObjectChangeEntriesCards
                    key={type}
                    type={type as SuiObjectChangeTypes}
                    data={changes}
                />
            ))}
        </>
    );
}
