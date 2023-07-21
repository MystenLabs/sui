// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import {
	ObjectChangeLabels,
	type SuiObjectChangeWithDisplay,
	type ObjectChangesByOwner,
	type ObjectChangeSummary,
	type SuiObjectChangeTypes,
	useResolveSuiNSName,
} from '@mysten/core';
import { ChevronRight12 } from '@mysten/icons';
import {
	type SuiObjectChangePublished,
	type SuiObjectChange,
	type DisplayFieldsResponse,
	parseStructTag,
} from '@mysten/sui.js';
import { Text } from '@mysten/ui';
import * as Collapsible from '@radix-ui/react-collapsible';
import clsx from 'clsx';
import { useState, type ReactNode } from 'react';

import { ObjectDisplay } from './ObjectDisplay';
import { ExpandableList, ExpandableListControl, ExpandableListItems } from '~/ui/ExpandableList';
import { AddressLink, ObjectLink } from '~/ui/InternalLink';
import { TransactionBlockCard, TransactionBlockCardSection } from '~/ui/TransactionBlockCard';

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
				label === ItemLabels.type ? 'items-start' : 'items-center',
			)}
		>
			<Text variant="pBody/medium" color="steel-dark">
				{label}
			</Text>

			{label === ItemLabels.package && packageId && <ObjectLink objectId={packageId} />}
			{label === ItemLabels.module && (
				<ObjectLink objectId={`${packageId}?module=${moduleName}`} label={moduleName} />
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
	headerContent,
}: {
	panelContent: ReactNode;
	headerContent?: ReactNode;
}) {
	const [open, setOpen] = useState(false);
	return (
		<Collapsible.Root open={open} onOpenChange={setOpen}>
			<div className="flex flex-wrap items-center justify-between">
				<Collapsible.Trigger>
					<div className="flex items-center gap-0.5">
						<Text variant="pBody/medium" color="steel-dark">
							Object
						</Text>

						<ChevronRight12 className={clsx('h-3 w-3 text-steel-dark', open && 'rotate-90')} />
					</div>
				</Collapsible.Trigger>
				{headerContent}
			</div>

			<Collapsible.Content>
				<div className="flex flex-col gap-2">{panelContent}</div>
			</Collapsible.Content>
		</Collapsible.Root>
	);
}

function ObjectDetail({
	objectType,
	objectId,
	display,
}: {
	objectType: string;
	objectId: string;
	display?: DisplayFieldsResponse;
}) {
	const separator = '::';
	const objectTypeSplit = objectType?.split(separator) || [];
	const typeName = objectTypeSplit.slice(2).join(separator);
	const { address, module, name } = parseStructTag(objectType);

	const objectDetailLabels = [ItemLabels.package, ItemLabels.module, ItemLabels.type];

	if (display?.data) return <ObjectDisplay display={display} objectId={objectId} />;

	return (
		<ObjectDetailPanel
			headerContent={
				<div className="flex items-center">
					<Text mono variant="body/medium" color="steel-dark">
						{name}:
					</Text>
					{objectId && <ObjectLink objectId={objectId} />}
				</div>
			}
			panelContent={
				<div className="mt-2 flex flex-col gap-2">
					{objectDetailLabels.map((label) => (
						<Item
							key={label}
							label={label}
							packageId={address}
							moduleName={module}
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
	changeEntries: SuiObjectChange[];
	isDisplay?: boolean;
}

function ObjectChangeEntries({ changeEntries, type, isDisplay }: ObjectChangeEntriesProps) {
	const title = ObjectChangeLabels[type];
	let expandableItems = [];

	if (type === 'published') {
		expandableItems = (changeEntries as SuiObjectChangePublished[]).map(
			({ packageId, modules }) => (
				<ObjectDetailPanel
					key={packageId}
					panelContent={
						<div className="mt-2 flex flex-col gap-2">
							<Item label={ItemLabels.package} packageId={packageId} />
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
			),
		);
	} else {
		expandableItems = (changeEntries as SuiObjectChangeWithDisplay[]).map((change) =>
			'objectId' in change && change.display ? (
				<ObjectDisplay key={change.objectId} objectId={change.objectId} display={change.display} />
			) : (
				'objectId' in change && (
					<ObjectDetail
						key={change.objectId}
						objectId={change.objectId}
						objectType={change.objectType}
						display={change.display}
					/>
				)
			),
		);
	}

	return (
		<TransactionBlockCardSection
			title={
				<Text
					variant="body/semibold"
					color={title === ObjectChangeLabels.created ? 'success-dark' : 'steel-darker'}
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
				<div
					className={clsx('flex max-h-[300px] gap-2 overflow-y-auto', {
						'flex-row': isDisplay,
						'flex-col': !isDisplay,
					})}
				>
					<ExpandableListItems />
				</div>

				{changeEntries.length > DEFAULT_ITEMS_TO_SHOW && (
					<div className="pt-4">
						<ExpandableListControl />
					</div>
				)}
			</ExpandableList>
		</TransactionBlockCardSection>
	);
}

interface ObjectChangeEntriesCardsProps {
	data: ObjectChangesByOwner;
	type: SuiObjectChangeTypes;
}

function ObjectChangeEntriesCardFooter({
	ownerType,
	ownerAddress,
}: {
	ownerType: string;
	ownerAddress: string;
}) {
	const { data: suinsDomainName } = useResolveSuiNSName(ownerAddress);

	return (
		<div className="flex flex-wrap items-center justify-between">
			<Text variant="pBody/medium" color="steel-dark">
				Owner
			</Text>

			{ownerType === 'AddressOwner' && (
				<AddressLink label={suinsDomainName || undefined} address={ownerAddress} />
			)}

			{ownerType === 'ObjectOwner' && <ObjectLink objectId={ownerAddress} />}

			{ownerType === 'Shared' && <ObjectLink objectId={ownerAddress} label="Shared" />}
		</div>
	);
}

export function ObjectChangeEntriesCards({ data, type }: ObjectChangeEntriesCardsProps) {
	if (!data) return null;

	return (
		<>
			{Object.entries(data).map(([ownerAddress, changes]) => {
				const renderFooter = ['AddressOwner', 'ObjectOwner', 'Shared'].includes(changes.ownerType);
				return (
					<TransactionBlockCard
						key={ownerAddress}
						title="Changes"
						size="sm"
						shadow
						footer={
							renderFooter && (
								<ObjectChangeEntriesCardFooter
									ownerType={changes.ownerType}
									ownerAddress={ownerAddress}
								/>
							)
						}
					>
						<div className="flex flex-col gap-4">
							{!!changes.changesWithDisplay.length && (
								<ObjectChangeEntries
									changeEntries={changes.changesWithDisplay}
									type={type}
									isDisplay
								/>
							)}
							{!!changes.changes.length && (
								<ObjectChangeEntries changeEntries={changes.changes} type={type} />
							)}
						</div>
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
				<ObjectChangeEntriesCards key={type} type={type as SuiObjectChangeTypes} data={changes} />
			))}
		</>
	);
}
