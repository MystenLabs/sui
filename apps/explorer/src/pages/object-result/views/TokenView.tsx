// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiObjectResponse } from '@mysten/sui.js/client';
import { Heading } from '@mysten/ui';
import { useState } from 'react';

import { DynamicFieldsCard } from '~/components/Object/DynamicFieldsCard';
import { ObjectFieldsCard } from '~/components/Object/ObjectFieldsCard';
import TransactionBlocksForAddress from '~/components/TransactionBlocksForAddress/TransactionBlocksForAddress';
import { Tabs, TabsList, TabsTrigger } from '~/ui/Tabs';

enum TABS_VALUES {
	FIELDS = 'fields',
	DYNAMIC_FIELDS = 'dynamicFields',
}

export function TokenView({ data }: { data: SuiObjectResponse }) {
	const [fieldsCount, setFieldsCount] = useState(0);

	const TABS = [
		{ label: 'Fields', value: TABS_VALUES.FIELDS, count: fieldsCount },
		{ label: 'Dynamic Fields', value: TABS_VALUES.DYNAMIC_FIELDS },
	];

	const [activeTab, setActiveTab] = useState<string>(TABS_VALUES.FIELDS);
	const objectId = data.data?.objectId!;

	return (
		<div className="flex flex-col flex-nowrap gap-14">
			<Tabs size="lg" value={activeTab} onValueChange={setActiveTab}>
				<TabsList>
					{TABS.map(({ label, value, count }) =>
						value !== TABS_VALUES.FIELDS || count ? (
							<TabsTrigger key={value} value={value}>
								<Heading variant="heading4/semibold">
									{count !== undefined ? count : ''} {label}
								</Heading>
							</TabsTrigger>
						) : null,
					)}
				</TabsList>
			</Tabs>
			<ObjectFieldsCard id={objectId} setFieldsCount={setFieldsCount} />
			<DynamicFieldsCard id={objectId} />
			<TransactionBlocksForAddress address={objectId} isObject />
		</div>
	);
}
