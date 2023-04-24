// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Combobox } from '@headlessui/react';
import { useGetObject, useGetNormalizedMoveStruct } from '@mysten/core';
import { Search24 } from '@mysten/icons';
import { getObjectFields, getObjectType } from '@mysten/sui.js';
import clsx from 'clsx';
import { useState } from 'react';

import { FieldItem } from './FieldItem';
import { ScrollToViewCard } from './ScrollToViewCard';
import { getFieldTypeValue } from './utils';

import { Banner } from '~/ui/Banner';
import { DisclosureBox } from '~/ui/DisclosureBox';
import { LoadingSpinner } from '~/ui/LoadingSpinner';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { Text } from '~/ui/Text';
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
    const [packageId, moduleName, functionName] =
        objectType?.split('<')[0]?.split('::') || [];

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
                <LoadingSpinner text="Loading data" />
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
                  name.toLowerCase().includes(query.toLowerCase())
              );

    // Return null if there are no fields
    if (!fieldsData || !normalizedStruct?.fields || !objectType) {
        return null;
    }

    return (
        <TabGroup size="lg">
            <TabList>
                <Tab>Fields</Tab>
            </TabList>
            <TabPanels>
                <TabPanel>
                    <div className="mt-4 flex flex-col gap-5 border-b border-gray-45">
                        <div className="flex flex-col gap-5  md:flex-row md:flex-nowrap">
                            <div className="w-full md:w-1/5">
                                <Combobox
                                    value={activeFieldName}
                                    onChange={setActiveFieldName}
                                >
                                    <div className="mt-2.5 flex w-full justify-between rounded-md border border-gray-50 py-1 pl-3 placeholder-gray-65 shadow-sm">
                                        <Combobox.Input
                                            onChange={(event) => {
                                                setQuery(event.target.value);
                                            }}
                                            displayValue={() => query}
                                            placeholder="Search"
                                            className="w-full border-none focus:outline-0"
                                        />
                                        <button
                                            className="border-none bg-inherit pr-2"
                                            type="submit"
                                        >
                                            <Search24 className="h-4.5 w-4.5 cursor-pointer fill-steel align-middle text-gray-60" />
                                        </button>
                                    </div>
                                    <Combobox.Options className="absolute left-0 z-10 flex h-fit max-h-verticalListLong w-full flex-col gap-1 overflow-auto rounded-md bg-white px-2 pb-5 pt-3 shadow-moduleOption md:left-auto md:w-1/6">
                                        {filteredFieldNames.length > 0 ? (
                                            <div className="ml-1.5 pb-2 text-caption font-semibold uppercase text-gray-75">
                                                {filteredFieldNames.length}
                                                {filteredFieldNames.length === 1
                                                    ? ' Result'
                                                    : ' Results'}
                                            </div>
                                        ) : (
                                            <div className="px-3.5 pt-2 text-center text-body italic text-gray-70">
                                                No results
                                            </div>
                                        )}
                                        {filteredFieldNames?.map(({ name }) => (
                                            <Combobox.Option
                                                key={name}
                                                value={name}
                                                className="list-none md:min-w-fit"
                                            >
                                                {({ active }) => (
                                                    <button
                                                        type="button"
                                                        className={clsx(
                                                            'mt-0.5 block w-full cursor-pointer rounded-md border px-1.5 py-2 text-left text-body',
                                                            active
                                                                ? 'border-transparent bg-gray-40 text-steel-darker'
                                                                : 'border-transparent bg-white font-medium text-steel-darker'
                                                        )}
                                                    >
                                                        {name}
                                                    </button>
                                                )}
                                            </Combobox.Option>
                                        ))}
                                    </Combobox.Options>
                                </Combobox>
                                <div className="max-h-600 overflow-y-auto overflow-x-clip py-3">
                                    <VerticalList>
                                        {normalizedStruct?.fields?.map(
                                            ({ name, type }) => (
                                                <div
                                                    key={name}
                                                    className="mt-0.5"
                                                >
                                                    <ListItem
                                                        active={
                                                            activeFieldName ===
                                                            name
                                                        }
                                                        onClick={() =>
                                                            setActiveFieldName(
                                                                name
                                                            )
                                                        }
                                                    >
                                                        <div className="flex w-full flex-1 justify-between gap-2 truncate">
                                                            <Text
                                                                variant="body/medium"
                                                                color="steel-darker"
                                                                truncate
                                                            >
                                                                {name}
                                                            </Text>

                                                            <Text
                                                                variant="pSubtitle/normal"
                                                                color="steel"
                                                                truncate
                                                            >
                                                                {
                                                                    getFieldTypeValue(
                                                                        type,
                                                                        objectType
                                                                    )
                                                                        .displayName
                                                                }
                                                            </Text>
                                                        </div>
                                                    </ListItem>
                                                </div>
                                            )
                                        )}
                                    </VerticalList>
                                </div>
                            </div>

                            <div className="grow overflow-auto border-gray-45 pt-1 md:w-3/5 md:border-l md:pl-7">
                                <div className="flex max-h-600 flex-col gap-5 overflow-y-auto pb-5">
                                    {normalizedStruct?.fields.map(
                                        ({ name, type }) => (
                                            <ScrollToViewCard
                                                key={name}
                                                inView={
                                                    name === activeFieldName
                                                }
                                            >
                                                <DisclosureBox
                                                    title={
                                                        <div className="min-w-fit max-w-[60%] truncate break-words text-body font-medium leading-relaxed text-steel-dark">
                                                            {name}:
                                                        </div>
                                                    }
                                                    preview={
                                                        <div className="flex items-center gap-1 truncate break-all">
                                                            {typeof fieldsData[
                                                                name
                                                            ] === 'object' ? (
                                                                <Text
                                                                    variant="body/medium"
                                                                    color="gray-90"
                                                                >
                                                                    Click to
                                                                    view
                                                                </Text>
                                                            ) : (
                                                                <FieldItem
                                                                    value={
                                                                        fieldsData[
                                                                            name
                                                                        ]
                                                                    }
                                                                    truncate
                                                                    type={type}
                                                                    objectType={
                                                                        objectType
                                                                    }
                                                                />
                                                            )}
                                                        </div>
                                                    }
                                                    variant="outline"
                                                >
                                                    <FieldItem
                                                        value={fieldsData[name]}
                                                        objectType={objectType}
                                                        type={type}
                                                    />
                                                </DisclosureBox>
                                            </ScrollToViewCard>
                                        )
                                    )}
                                </div>
                            </div>
                        </div>
                    </div>
                </TabPanel>
            </TabPanels>
        </TabGroup>
    );
}
