// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Combobox } from '@headlessui/react';
import { getObjectFields } from '@mysten/sui.js';
import clsx from 'clsx';
import { useState } from 'react';

import { FieldItem } from './FieldItem';

import { ReactComponent as SearchIcon } from '~/assets/SVGIcons/24px/Search.svg';
import { useGetObject } from '~/hooks/useGetObject';
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

    if (isLoading) {
        return (
            <div className="flex w-full justify-center">
                <LoadingSpinner text="Loading data" />
            </div>
        );
    }
    if (isError) {
        return null;
    }

    const fieldsData = getObjectFields(data!);
    const fieldsNames = Object.entries(fieldsData || {});
    const filteredFieldNames =
        query === ''
            ? fieldsNames
            : fieldsNames
                  .filter(([name]) =>
                      name.toLowerCase().includes(query.toLowerCase())
                  )
                  .map((name) => name);

    // TODO scroll to active field
    const onChangeField = (newKeyName: string) => {
        setActiveFieldName(newKeyName);
    };

    return fieldsNames?.length && fieldsData ? (
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
                                            <SearchIcon className="h-4.5 w-4.5 cursor-pointer fill-steel align-middle" />
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
                                        {filteredFieldNames?.map(([name]) => (
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
                                <div className="max-h-[600px] min-h-full overflow-auto overflow-x-scroll py-3">
                                    <VerticalList>
                                        {fieldsNames.map(([name, value]) => (
                                            <div
                                                key={name}
                                                className="mx-0.5 mt-0.5 md:min-w-fit"
                                            >
                                                <ListItem
                                                    active={
                                                        activeFieldName === name
                                                    }
                                                    onClick={() =>
                                                        onChangeField(name)
                                                    }
                                                >
                                                    <div className="flex justify-between">
                                                        <Text
                                                            variant="body/medium"
                                                            color="steel-darker"
                                                        >
                                                            {name}
                                                        </Text>
                                                        <div className="capitalize">
                                                            <Text
                                                                variant="subtitle/normal"
                                                                color="steel"
                                                            >
                                                                {typeof value}
                                                            </Text>
                                                        </div>
                                                    </div>
                                                </ListItem>
                                            </div>
                                        ))}
                                    </VerticalList>
                                </div>
                            </div>

                            <div className="grow overflow-auto border-gray-45 pt-1 md:w-3/5 md:border-l md:pl-7">
                                <div className="flex max-h-[600px] flex-col gap-5 overflow-x-scroll pb-5">
                                    {Object.entries(fieldsData).map(
                                        ([key, value]) => (
                                            <div key={key}>
                                                <DisclosureBox
                                                    title={
                                                        <Text
                                                            variant="body/medium"
                                                            color="steel-dark"
                                                        >
                                                            {key.toString()}:
                                                        </Text>
                                                    }
                                                    preview={
                                                        <div className="flex items-end gap-1 truncate break-all">
                                                            {typeof value ===
                                                            'object' ? (
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
                                                                        value
                                                                    }
                                                                    type={key}
                                                                />
                                                            )}
                                                        </div>
                                                    }
                                                    variant="outline"
                                                >
                                                    <FieldItem
                                                        value={value}
                                                        type={key}
                                                    />
                                                </DisclosureBox>
                                            </div>
                                        )
                                    )}
                                </div>
                            </div>
                        </div>
                    </div>
                </TabPanel>
            </TabPanels>
        </TabGroup>
    ) : null;
}
