// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useGetObject } from '@mysten/core';
import { useNormalizedMoveStruct } from '@mysten/dapp-kit';
import { Search24 } from '@mysten/icons';
import { Text, LoadingIndicator, Combobox, ComboboxInput, ComboboxList } from '@mysten/ui';
import { useCallback, useEffect, useRef, useState } from 'react';

import { FieldItem } from './FieldItem';
import { ScrollToViewCard } from './ScrollToViewCard';
import { getFieldTypeValue } from './utils';
import { FieldsCard, FieldCollapsible, FieldsContainer } from '~/components/Object/FieldsUtils';
import { Banner } from '~/ui/Banner';
import { DescriptionItem } from '~/ui/DescriptionList';

interface ObjectFieldsProps {
	id: string;
	setCount: (count: number) => void;
}

export function ObjectFieldsCard({ id, setCount }: ObjectFieldsProps) {
	const { data, isLoading, isError } = useGetObject(id);
	const [query, setQuery] = useState('');
	const [activeFieldName, setActiveFieldName] = useState('');
	const [openFieldsName, setOpenFieldsName] = useState<{
		[name: string]: boolean;
	}>({});

	const objectType =
		data?.data?.type ?? data?.data?.content?.dataType === 'package'
			? data.data.type
			: data?.data?.content?.type;

	// Get the packageId, moduleName, functionName from the objectType
	const [packageId, moduleName, functionName] = objectType?.split('<')[0]?.split('::') || [];
	const containerRef = useRef<HTMLDivElement>(null);

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
			setOpenFieldsName(
				normalizedStruct.fields.reduce(
					(acc, { name }) => {
						acc[name] = false;
						return acc;
					},
					{} as { [name: string]: boolean },
				),
			);
		}
	}, [normalizedStruct?.fields]);

	const onSetOpenFieldsName = useCallback(
		(name: string) => (open: boolean) => {
			setOpenFieldsName((prev) => ({
				...prev,
				[name]: open,
			}));
		},
		[],
	);

	const onFieldsNameClick = useCallback(
		(name: string) => {
			setActiveFieldName(name);
			onSetOpenFieldsName(name)(true);
		},
		[onSetOpenFieldsName],
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
		<FieldsContainer>
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
				<div className="mt-4 flex h-80 flex-col gap-4 overflow-y-auto pl-3 pr-2">
					{normalizedStruct?.fields?.map(({ name, type }) => (
						<button
							type="button"
							key={name}
							className="mt-0.5"
							onClick={() => onFieldsNameClick(name)}
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
			</div>

			<FieldsCard ref={containerRef}>
				{normalizedStruct?.fields.map(({ name, type }, index) => (
					<ScrollToViewCard
						key={name}
						inView={name === activeFieldName}
						containerRef={containerRef}
					>
						<FieldCollapsible
							open={openFieldsName[name]}
							setOpen={onSetOpenFieldsName(name)}
							name={name}
							noMarginBottom={index === normalizedStruct?.fields.length - 1}
						>
							<FieldItem value={fieldsData[name]} objectType={objectType} type={type} />
						</FieldCollapsible>
					</ScrollToViewCard>
				))}
			</FieldsCard>
		</FieldsContainer>
	);
}
