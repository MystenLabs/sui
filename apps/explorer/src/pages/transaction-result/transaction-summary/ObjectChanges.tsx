// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Disclosure } from '@headlessui/react';
import {
    getGroupByOwner,
    LocationIdType,
    type ObjectChangeSummary,
    useGetNFTMeta,
} from '@mysten/core';
import { ChevronRight12 } from '@mysten/icons';
import {
    type SuiObjectChangeCreated,
    type SuiObjectChangeMutated,
    type SuiObjectChangePublished,
    type SuiObjectChangeTransferred,
} from '@mysten/sui.js';
import clsx from 'clsx';
import { type ReactNode, useState } from 'react';

import {
    ExpandableList,
    ExpandableListControl,
    ExpandableListItems,
} from '~/ui/ExpandableList';
import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { ImageModal } from '~/ui/Modal/ImageModal';
import { Text } from '~/ui/Text';
import {
    TransactionBlockCard,
    TransactionBlockCardSection,
} from '~/ui/TransactionBlockCard';
import { Image } from '~/ui/image/Image';

enum Labels {
    created = 'Created',
    mutated = 'Updated',
    transferred = 'Transfer',
    published = 'Publish',
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

interface ItemProps {
    label: ItemLabels;
    packageId?: string;
    moduleName?: string;
    typeName?: string;
}

function Item({ label, packageId, moduleName, typeName }: ItemProps) {
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

interface NFTItemProps {
    description: string;
    imageUrl: string;
    objectId: string;
}

function NFTItem({ description, imageUrl, objectId }: NFTItemProps) {
    const [open, handleOpen] = useState(false);

    return (
        <>
            <ImageModal
                open={open}
                onClose={() => handleOpen(false)}
                title={description}
                subtitle={description}
                src={imageUrl}
                alt={description}
            />
            <div className="relative w-32 cursor-pointer whitespace-nowrap">
                <Image
                    size="lg"
                    rounded="2xl"
                    src={imageUrl!}
                    alt={description}
                    onClick={() => handleOpen(true)}
                />
                <div className="absolute bottom-2 left-1/2 flex -translate-x-1/2 justify-center rounded-lg bg-white px-2 py-1">
                    <ObjectLink objectId={objectId} />
                </div>
            </div>
        </>
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

    const nftMetaData = useGetNFTMeta(objectId);

    const isNFT = !!nftMetaData.data;

    const objectDetailLabels = [
        ItemLabels.package,
        ItemLabels.module,
        ItemLabels.type,
    ];

    if (isNFT) {
        return (
            <NFTItem
                objectId={objectId}
                description={nftMetaData.data?.description!}
                imageUrl={nftMetaData.data?.imageUrl!}
            />
        );
    }

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

interface ObjectChangeEntriesProps extends ObjectChangeEntryBaseProps {
    changeEntries: (
        | SuiObjectChangeMutated
        | SuiObjectChangeCreated
        | SuiObjectChangePublished
    )[];
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

interface ObjectChangeEntriesCardsProps extends ObjectChangeEntryBaseProps {
    data:
        | ObjectChangeEntryData<SuiObjectChangeMutated>
        | ObjectChangeEntryData<SuiObjectChangeTransferred>
        | ObjectChangeEntryData<SuiObjectChangeCreated>;
}

export function ObjectChangeEntriesCards({
    data,
    type,
}: ObjectChangeEntriesCardsProps) {
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
                                <div className="flex flex-wrap items-center justify-between">
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
    objectSummary: ObjectChangeSummary;
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
                <ObjectChangeEntriesCards
                    type="created"
                    data={
                        createdChangesByOwner as unknown as ObjectChangeEntryData<SuiObjectChangeCreated>
                    }
                />
            ) : null}

            {objectSummary.mutated?.length ? (
                <ObjectChangeEntriesCards
                    type="mutated"
                    data={
                        updatedChangesByOwner as unknown as ObjectChangeEntryData<SuiObjectChangeMutated>
                    }
                />
            ) : null}

            {objectSummary.transferred?.length ? (
                <ObjectChangeEntriesCards
                    type="transferred"
                    data={
                        transferredChangesByOwner as unknown as ObjectChangeEntryData<SuiObjectChangeTransferred>
                    }
                />
            ) : null}

            {objectSummary.published?.length ? (
                <TransactionBlockCard title="Changes" size="sm" shadow>
                    <ObjectChangeEntries
                        changeEntries={objectSummary.published}
                        type="published"
                    />
                </TransactionBlockCard>
            ) : null}
        </>
    );
}
