// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { Disclosure } from '@headlessui/react';
import {
	getObjectChangeLabel,
	type ObjectChangesByOwner,
	type ObjectChangeSummary,
	type SuiObjectChangeWithDisplay,
	type SuiObjectChangeTypes,
} from '@mysten/core';
import { ChevronDown12, ChevronRight12 } from '@mysten/icons';
import {
	SuiObjectChangeTransferred,
	formatAddress,
	is,
	SuiObjectChangePublished,
} from '@mysten/sui.js';
import cx from 'classnames';

import { ObjectChangeDisplay } from './objectSummary/ObjectChangeDisplay';
import { ExpandableList } from '../../ExpandableList';
import { Card } from '../Card';
import { OwnerFooter } from '../OwnerFooter';
import ExplorerLink from '_src/ui/app/components/explorer-link';
import { ExplorerLinkType } from '_src/ui/app/components/explorer-link/ExplorerLinkType';
import { Text } from '_src/ui/app/shared/text';

function ChevronDown({ expanded }: { expanded: boolean }) {
	return expanded ? (
		<ChevronDown12 className="text-gray-45" />
	) : (
		<ChevronRight12 className="text-gray-45" />
	);
}

export function ObjectDetail({
	change,
	display,
}: {
	change: SuiObjectChangeWithDisplay;
	ownerKey: string;
	display?: boolean;
}) {
	if (is(change, SuiObjectChangeTransferred) || is(change, SuiObjectChangePublished)) {
		return null;
	}
	const [packageId, moduleName, typeName] = change.objectType?.split('<')[0]?.split('::') || [];

	return (
		<Disclosure>
			{({ open }) => (
				<div className="flex flex-col gap-1">
					<div className="grid grid-cols-2 overflow-auto cursor-pointer">
						<Disclosure.Button className="flex items-center cursor-pointer border-none bg-transparent ouline-none p-0 gap-1 text-steel-dark hover:text-steel-darker select-none">
							<Text variant="pBody" weight="medium">
								Object
							</Text>
							{open ? (
								<ChevronDown12 className="text-gray-45" />
							) : (
								<ChevronRight12 className="text-gray-45" />
							)}
						</Disclosure.Button>
						{change.objectId && (
							<div className="justify-self-end">
								<ExplorerLink
									type={ExplorerLinkType.object}
									objectID={change.objectId}
									className="text-hero-dark no-underline"
								>
									<Text variant="body" weight="medium" truncate mono>
										{formatAddress(change.objectId)}
									</Text>
								</ExplorerLink>
							</div>
						)}
					</div>
					<Disclosure.Panel>
						<div className="flex flex-col gap-1">
							<div className="grid grid-cols-2 overflow-auto relative">
								<Text variant="pBody" weight="medium" color="steel-dark">
									Package
								</Text>
								<div className="flex justify-end">
									<ExplorerLink
										type={ExplorerLinkType.object}
										objectID={packageId}
										className="text-hero-dark text-captionSmall no-underline justify-self-end overflow-auto"
									>
										<Text variant="pBody" weight="medium" truncate mono>
											{packageId}
										</Text>
									</ExplorerLink>
								</div>
							</div>
							<div className="grid grid-cols-2 overflow-auto">
								<Text variant="pBody" weight="medium" color="steel-dark">
									Module
								</Text>
								<div className="flex justify-end">
									<ExplorerLink
										type={ExplorerLinkType.object}
										objectID={packageId}
										moduleName={moduleName}
										className="text-hero-dark no-underline justify-self-end overflow-auto"
									>
										<Text variant="pBody" weight="medium" truncate mono>
											{moduleName}
										</Text>
									</ExplorerLink>
								</div>
							</div>
							<div className="grid grid-cols-2 overflow-auto">
								<Text variant="pBody" weight="medium" color="steel-dark">
									Type
								</Text>
								<div className="flex justify-end">
									<ExplorerLink
										type={ExplorerLinkType.object}
										objectID={packageId}
										moduleName={moduleName}
										className="text-hero-dark no-underline justify-self-end overflow-auto"
									>
										<Text variant="pBody" weight="medium" truncate mono>
											{typeName}
										</Text>
									</ExplorerLink>
								</div>
							</div>
						</div>
					</Disclosure.Panel>
				</div>
			)}
		</Disclosure>
	);
}

interface ObjectChangeEntryProps {
	type: SuiObjectChangeTypes;
	changes: ObjectChangesByOwner;
}

export function ObjectChangeEntry({ changes, type }: ObjectChangeEntryProps) {
	return (
		<>
			{Object.entries(changes).map(([owner, changes]) => {
				return (
					<Card
						footer={<OwnerFooter owner={owner} ownerType={changes.ownerType} />}
						key={`${type}-${owner}`}
						heading="Changes"
					>
						<Disclosure defaultOpen>
							{({ open }) => (
								<div className={cx({ 'gap-4': open }, 'flex flex-col pb-3')}>
									<Disclosure.Button as="div" className="flex w-full flex-col gap-2 cursor-pointer">
										<div className="flex w-full items-center gap-2">
											<Text
												variant="body"
												weight="semibold"
												color={type === 'created' ? 'success-dark' : 'steel-darker'}
											>
												{getObjectChangeLabel(type)}
											</Text>
											<div className="h-px bg-gray-40 w-full" />
											<ChevronDown expanded={open} />
										</div>
									</Disclosure.Button>
									<Disclosure.Panel as="div" className="gap-4 flex flex-col">
										<>
											{!!changes.changesWithDisplay.length && (
												<div className="flex gap-2 overflow-y-auto">
													<ExpandableList
														defaultItemsToShow={5}
														items={
															open
																? changes.changesWithDisplay.map((change) => (
																		<ObjectChangeDisplay change={change} />
																  ))
																: []
														}
													/>
												</div>
											)}

											<div className="flex w-full flex-col gap-2">
												<ExpandableList
													defaultItemsToShow={5}
													items={
														open
															? changes.changes.map((change) => (
																	<ObjectDetail ownerKey={owner} change={change} />
															  ))
															: []
													}
												/>
											</div>
										</>
									</Disclosure.Panel>
								</div>
							)}
						</Disclosure>
					</Card>
				);
			})}
		</>
	);
}

export function ObjectChanges({ changes }: { changes?: ObjectChangeSummary | null }) {
	if (!changes) return null;

	return (
		<>
			{Object.entries(changes).map(([type, changes]) => {
				return (
					<ObjectChangeEntry
						key={type}
						type={type as keyof ObjectChangeSummary}
						changes={changes}
					/>
				);
			})}
		</>
	);
}
