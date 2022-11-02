// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Combobox } from '@headlessui/react';
import { useState, useEffect, useMemo } from 'react';
import { useSearchParams } from 'react-router-dom';

import ModuleView from './ModuleView';

import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { ListItem, VerticalList } from '~/ui/VerticalList';

interface Props {
    id?: string;
    modules: [moduleName: string, code: string][];
}

const initialSelectModule = (searchParams, modulenames: string[]) => {
    const paramModule = searchParams.get('module') || null;

    if (!!paramModule && modulenames.includes(paramModule)) {
        return paramModule;
    } else {
        return modulenames[0];
    }
};

function PkgModuleViewWrapper({ id, modules }: Props) {
    const modulenames = useMemo(
        () => modules.map(([name], idx) => name),
        [modules]
    );
    const [searchParams, setSearchParams] = useSearchParams();
    const [query, setQuery] = useState('');
    const [selectedModule, setSelectedModule] = useState(
        initialSelectModule(searchParams, modulenames)
    );

    const filteredModules =
        query === ''
            ? modulenames
            : modules
                  .filter(([name]) => name.startsWith(query))
                  .map(([name]) => name);

    useEffect(() => {
        setSearchParams({ module: selectedModule });
    }, [selectedModule, setSearchParams]);

    return (
        <div
            className={
                'flex flex-wrap border-0 border-y border-solid border-sui-grey-45'
            }
        >
            <div
                className={
                    'h-[605px] w-full lg:w-[15vw] overflow-auto pt-[10px] pr-[20px] pl-[1px]'
                }
            >
                <Combobox
                    value={selectedModule}
                    onChange={setSelectedModule}
                    nullable
                >
                    <Combobox.Input
                        onChange={(event) => setQuery(event.target.value)}
                        displayValue={() => ''}
                    />
                    <Combobox.Options static as="div">
                        <VerticalList>
                            {filteredModules.map((name, idx) => (
                                <Combobox.Option key={name} value={name}>
                                    {({ active }) => (
                                        <div
                                            key={idx}
                                            className="w-full lg:min-w-[12vw] lg:w-fit"
                                        >
                                            <ListItem
                                                active={
                                                    active ||
                                                    selectedModule === name
                                                }
                                            >
                                                {name}
                                            </ListItem>
                                        </div>
                                    )}
                                </Combobox.Option>
                            ))}
                        </VerticalList>
                    </Combobox.Options>
                </Combobox>
            </div>
            <div className="border-0 lg:border-l border-solid border-sui-grey-45 lg:pl-[30px] pt-[20px]">
                <TabGroup size="md">
                    <TabList>
                        <Tab>Bytecode</Tab>
                    </TabList>
                    <TabPanels>
                        <TabPanel>
                            <div className="overflow-auto h-[555px] w-[87vw] lg:w-[75vw]">
                                {modules
                                    .filter(([name]) => name === selectedModule)
                                    .map(([name, code], idx) => (
                                        <ModuleView
                                            key={idx}
                                            id={id}
                                            name={name}
                                            code={code}
                                        />
                                    ))}
                            </div>
                        </TabPanel>
                    </TabPanels>
                </TabGroup>
            </div>
        </div>
    );
}
export default PkgModuleViewWrapper;
