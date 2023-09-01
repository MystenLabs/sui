// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetObject } from '@mysten/core';
import { useNormalizedMoveStruct } from '@mysten/dapp-kit';
import { Search24 } from '@mysten/icons';
import { type SuiMoveNormalizedType } from '@mysten/sui.js/client';
import { Text, LoadingIndicator, Combobox, ComboboxInput, ComboboxList, Button } from '@mysten/ui';
import clsx from 'clsx';
import { type ReactNode, useEffect, useState } from 'react';

import { FieldItem } from './FieldItem';
import { ScrollToViewCard } from './ScrollToViewCard';
import { getFieldTypeValue } from './utils';
import { FieldsCollapsible } from '~/components/Object/FieldsCollapsible';
import { Banner } from '~/ui/Banner';
import { Card } from '~/ui/Card';
import { DescriptionItem } from '~/ui/DescriptionList';
import { DisclosureBox } from '~/ui/DisclosureBox';
import { TabHeader } from '~/ui/Tabs';
import { ListItem, VerticalList } from '~/ui/VerticalList';
import { CollapsibleSection } from '~/ui/collapsible/CollapsibleSection';

interface ObjectFieldsProps {
	id: string;
	setCount: (count: number) => void;
}

export function ObjectFieldsCard({ id, setCount }: ObjectFieldsProps) {
	const { data, isLoading, isError } = useGetObject(id);
	const [query, setQuery] = useState('');
	const [activeFieldName, setActiveFieldName] = useState('');
	const objectType =
		data?.data?.type ?? data?.data?.content?.dataType === 'package'
			? data.data.type
			: data?.data?.content?.type;

	// Get the packageId, moduleName, functionName from the objectType
	const [packageId, moduleName, functionName] = objectType?.split('<')[0]?.split('::') || [];

	// Get the normalized struct for the object
	const {
		data: normalizedStruct,
		isLoading: loadingNormalizedStruct,
		isError: errorNormalizedMoveStruct,
	} = useNormalizedMoveStruct(
		{
			package: packageId,
			module: moduleName,
			struct: functionName,
		},
		{
			enabled: !!packageId && !!moduleName && !!functionName,
			onSuccess: (data) => {
				if (data?.fields && activeFieldName === '') {
					setActiveFieldName(data.fields[0].name);
				}
			},
		},
	);

	useEffect(() => {
		if (normalizedStruct?.fields) {
			setCount(normalizedStruct.fields.length);
		}
	}, [normalizedStruct?.fields, setCount]);

	if (isLoading || loadingNormalizedStruct) {
		return (
			<div className="flex w-full justify-center">
				<LoadingIndicator text="Loading data" />
			</div>
		);
	}
	if (isError || errorNormalizedMoveStruct) {
		return (
			<Banner variant="error" spacing="lg" fullWidth>
				Failed to get field data for {id}
			</Banner>
		);
	}

	const fieldsData =
		data.data?.content?.dataType === 'moveObject'
			? (data.data?.content?.fields as Record<string, string | number | object>)
			: null;

	const filteredFieldNames =
		query === ''
			? normalizedStruct?.fields
			: normalizedStruct?.fields.filter(({ name }) =>
					name.toLowerCase().includes(query.toLowerCase()),
			  );

	// Return null if there are no fields
	if (!fieldsData || !normalizedStruct?.fields || !objectType) {
		return null;
	}

	return (
		<div className="flex flex-col gap-10 md:flex-row md:flex-nowrap">
			<div className="w-full md:w-1/5">
				<Combobox value={query} onValueChange={setQuery}>
					<div className="flex w-full justify-between rounded-lg border border-white/50 bg-white py-1 pl-3 shadow-dropdownContent">
						<ComboboxInput placeholder="Search" className="w-full border-none focus:outline-0" />
						<button className="border-none bg-inherit pr-2" type="submit">
							<Search24 className="h-4.5 w-4.5 cursor-pointer fill-steel align-middle text-gray-60" />
						</button>
					</div>
					<ComboboxList
						showResultsCount
						options={filteredFieldNames.map((item) => ({
							value: item.name,
							label: item.name,
						}))}
						onSelect={({ value }) => {
							setActiveFieldName(value);
						}}
					/>
				</Combobox>
				<div className="max-h-600 overflow-y-auto overflow-x-clip py-4.5">
					<VerticalList>
						<div className="flex flex-col gap-4 pl-3">
							{normalizedStruct?.fields?.map(({ name, type }) => (
								<button
									type="button"
									key={name}
									className="mt-0.5"
									onClick={() => setActiveFieldName(name)}
								>
									<DescriptionItem
										descriptionJustify="end"
										labelWidth="md"
										title={
											<Text variant="body/medium" color="steel-darker">
												{name}
											</Text>
										}
									>
										<Text uppercase variant="subtitle/normal" color="steel" truncate>
											{getFieldTypeValue(type, objectType).displayName}
										</Text>
									</DescriptionItem>
								</button>
							))}
						</div>
					</VerticalList>
				</div>
			</div>

			<div className="h-100 w-full overflow-auto rounded-xl border-transparent bg-transparent px-2">
				<Card shadow bg="white">
					{normalizedStruct?.fields.map(({ name, type }) => (
						<ScrollToViewCard key={name} inView={name === activeFieldName}>
							<FieldsCollapsible name={name}>
								<FieldItem value={fieldsData[name]} objectType={objectType} type={type} />
							</FieldsCollapsible>
						</ScrollToViewCard>
					))}
				</Card>
			</div>
		</div>
	);
}
