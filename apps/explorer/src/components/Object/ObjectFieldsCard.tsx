// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetObject, useGetNormalizedMoveStruct } from '@mysten/core';
import { Search24 } from '@mysten/icons';
import { getObjectFields, getObjectType } from '@mysten/sui.js';
import { Text, LoadingIndicator, Combobox, ComboboxInput, ComboboxList } from '@mysten/ui';
import { useState } from 'react';

import { FieldItem } from './FieldItem';
import { ScrollToViewCard } from './ScrollToViewCard';
import { getFieldTypeValue } from './utils';
import { Banner } from '~/ui/Banner';
import { DisclosureBox } from '~/ui/DisclosureBox';
import { TabHeader } from '~/ui/Tabs';
import { ListItem, VerticalList } from '~/ui/VerticalList';

interface ObjectFieldsProps {
	id: string;
}

export function ObjectFieldsCard({ id }: ObjectFieldsProps) {
	const { data, isLoading, isError } = useGetObject(id);
	const [query, setQuery] = useState('');
	const [activeFieldName, setActiveFieldName] = useState('');
	const objectType = getObjectType(data!);

	// Get the packageId, moduleName, functionName from the objectType
	const [packageId, moduleName, functionName] = objectType?.split('<')[0]?.split('::') || [];

	// Get the normalized struct for the object
	const {
		data: normalizedStruct,
		isLoading: loadingNormalizedStruct,
		isError: errorNormalizedMoveStruct,
	} = useGetNormalizedMoveStruct({
		packageId,
		module: moduleName,
		struct: functionName,
		onSuccess: (data) => {
			if (data?.fields && activeFieldName === '') {
				setActiveFieldName(data.fields[0].name);
			}
		},
	});

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

	const fieldsData = getObjectFields(data!);

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
		<TabHeader title="Fields">
			<div className="mt-4 flex flex-col gap-5 border-b border-gray-45">
				<div className="flex flex-col gap-5  md:flex-row md:flex-nowrap">
					<div className="w-full md:w-1/5">
						<Combobox value={query} onValueChange={setQuery}>
							<div className="mt-2.5 flex w-full justify-between rounded-md border border-gray-50 py-1 pl-3 placeholder-gray-65 shadow-sm">
								<ComboboxInput
									placeholder="Search"
									className="w-full border-none focus:outline-0"
								/>
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
						<div className="max-h-600 overflow-y-auto overflow-x-clip py-3">
							<VerticalList>
								{normalizedStruct?.fields?.map(({ name, type }) => (
									<div key={name} className="mt-0.5">
										<ListItem
											active={activeFieldName === name}
											onClick={() => setActiveFieldName(name)}
										>
											<div className="flex w-full flex-1 justify-between gap-2 truncate">
												<Text variant="body/medium" color="steel-darker" truncate>
													{name}
												</Text>

												<Text variant="pSubtitle/normal" color="steel" truncate>
													{getFieldTypeValue(type, objectType).displayName}
												</Text>
											</div>
										</ListItem>
									</div>
								))}
							</VerticalList>
						</div>
					</div>

					<div className="grow overflow-auto border-gray-45 pt-1 md:w-3/5 md:border-l md:pl-7">
						<div className="flex max-h-600 flex-col gap-5 overflow-y-auto pb-5">
							{normalizedStruct?.fields.map(({ name, type }) => (
								<ScrollToViewCard key={name} inView={name === activeFieldName}>
									<DisclosureBox
										title={
											<div className="min-w-fit max-w-[60%] truncate break-words text-body font-medium leading-relaxed text-steel-dark">
												{name}:
											</div>
										}
										preview={
											<div className="flex items-center gap-1 truncate break-all">
												{typeof fieldsData[name] === 'object' ? (
													<Text variant="body/medium" color="gray-90">
														Click to view
													</Text>
												) : (
													<FieldItem
														value={fieldsData[name]}
														truncate
														type={type}
														objectType={objectType}
													/>
												)}
											</div>
										}
										variant="outline"
									>
										<FieldItem value={fieldsData[name]} objectType={objectType} type={type} />
									</DisclosureBox>
								</ScrollToViewCard>
							))}
						</div>
					</div>
				</div>
			</div>
		</TabHeader>
	);
}
