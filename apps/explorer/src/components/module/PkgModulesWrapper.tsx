// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Combobox } from '@headlessui/react';
import clsx from 'clsx';
import { useState, useEffect, useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';

import ModuleView from './ModuleView';

import { ReactComponent as SearchIcon } from '~/assets/SVGIcons/24px/Search.svg';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { ListItem, VerticalList } from '~/ui/VerticalList';

type ModuleType = [moduleName: string, code: string];

interface Props {
    id?: string;
    modules: ModuleType[];
}

interface ModuleViewWrapperProps {
    id?: string;
    selectedModuleName: string;
    modules: ModuleType[];
}

function ModuleViewWrapper({
    id,
    selectedModuleName,
    modules,
}: ModuleViewWrapperProps) {
    const selectedModuleData = modules.find(
        ([name]) => name === selectedModuleName
    );

    if (!selectedModuleData) {
        return null;
    }

    const [name, code] = selectedModuleData;

    return <ModuleView id={id} name={name} code={code} />;
}

function PkgModuleViewWrapper({ id, modules }: Props) {
    const modulenames = modules.map(([name]) => name);
    const [searchParams, setSearchParams] = useSearchParams();
    const [query, setQuery] = useState('');

    const [selectedModule, setSelectedModule] = useState(modulenames[0]);

    const filteredModules =
        query === ''
            ? modulenames
            : modules
                  .filter(([name]) =>
                      name.toLowerCase().includes(query.toLowerCase())
                  )
                  .map(([name]) => name);

    useEffect(() => {
        const paramModule =
            searchParams.get('module') || modulenames?.[0] || null;
        setSelectedModule(
            !!paramModule && modulenames.includes(paramModule)
                ? paramModule
                : modulenames[0]
        );
    }, [searchParams, modulenames]);

    const updateSelectedModule = useCallback(
        (newModule: string) => () => {
            const newSearchParams = new URLSearchParams(searchParams);
            newSearchParams.set('module', newModule);
            setSearchParams(newSearchParams);
            setSelectedModule(newModule);
        },
        [searchParams, setSearchParams]
    );

    const submitSearch = useCallback(() => {
        if (filteredModules.length === 1)
            updateSelectedModule(filteredModules[0]);
    }, [filteredModules, updateSelectedModule]);

    return (
        <div className="flex flex-col md:flex-row md:flex-nowrap gap-5 border-0 border-y border-solid border-sui-grey-45">
            <div className="w-full md:w-1/5">
                <Combobox
                    value={selectedModule}
                    onChange={updateSelectedModule}
                >
                    <div className="box-border border border-sui-grey-50 border-solid rounded-md shadow-sm placeholder-sui-grey-65 pl-3 w-full flex mt-2.5 justify-between py-1">
                        <Combobox.Input
                            onChange={(event) => setQuery(event.target.value)}
                            displayValue={() => query}
                            placeholder="Search"
                            className="border-none w-full"
                        />
                        <button
                            onClick={submitSearch}
                            className="bg-inherit border-none pr-2"
                            type="submit"
                        >
                            <SearchIcon className="fill-sui-steel cursor-pointer h-4.5 w-4.5 align-middle" />
                        </button>
                    </div>
                    <Combobox.Options className="overflow-auto absolute left-0 w-full box-border md:w-1/6 md:left-auto h-fit max-h-verticalListLong overflow-auto shadow-moduleOption rounded-md bg-white z-10 px-2 pb-5 pt-3 flex flex-col gap-1">
                        {filteredModules.length > 0 ? (
                            <div className="text-caption font-semibold ml-1.5 pb-2 uppercase text-sui-grey-75">
                                {filteredModules.length}
                                {filteredModules.length === 1
                                    ? ' Result'
                                    : ' Results'}
                            </div>
                        ) : (
                            <div className="text-sui-grey-70 text-body italic pt-2 px-3.5 text-center">
                                No results
                            </div>
                        )}
                        {filteredModules.map((name) => (
                            <Combobox.Option
                                key={name}
                                value={name}
                                className="list-none md:min-w-fit"
                            >
                                {({ active }) => (
                                    <button
                                        type="button"
                                        className={clsx(
                                            'cursor-pointer py-2 rounded-md text-body block w-full text-left mt-0.5 px-1.5 border',
                                            active
                                                ? 'bg-sui/10 text-sui-grey-80 border-transparent'
                                                : 'bg-white text-sui-grey-80 font-medium border-transparent'
                                        )}
                                    >
                                        {name}
                                    </button>
                                )}
                            </Combobox.Option>
                        ))}
                    </Combobox.Options>
                </Combobox>
                <div className="h-verticalListShort md:h-verticalListLong overflow-auto pt-3">
                    <VerticalList>
                        {modulenames.map((name) => (
                            <div
                                key={name}
                                className="md:min-w-fit mx-0.5 mt-0.5"
                            >
                                <ListItem
                                    active={selectedModule === name}
                                    onClick={updateSelectedModule(name)}
                                >
                                    {name}
                                </ListItem>
                            </div>
                        ))}
                    </VerticalList>
                </div>
            </div>
            <div className="border-0 md:border-l border-solid border-sui-grey-45 md:pl-7 pt-5 grow overflow-auto">
                <TabGroup size="md">
                    <TabList>
                        <Tab>Bytecode</Tab>
                    </TabList>
                    <TabPanels>
                        <TabPanel>
                            <div className="overflow-auto h-verticalListLong">
                                <ModuleViewWrapper
                                    id={id}
                                    modules={modules}
                                    selectedModuleName={selectedModule}
                                />
                            </div>
                        </TabPanel>
                    </TabPanels>
                </TabGroup>
            </div>
        </div>
    );
}
export default PkgModuleViewWrapper;
