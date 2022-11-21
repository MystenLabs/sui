// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useFeature } from '@growthbook/growthbook-react';
import { Combobox } from '@headlessui/react';
import clsx from 'clsx';
import { useState, useCallback, useEffect } from 'react';
import { useSearchParams } from 'react-router-dom';

import ModuleView from './ModuleView';
import { ModuleFunctionsInteraction } from './module-functions-interaction';

import { ReactComponent as SearchIcon } from '~/assets/SVGIcons/24px/Search.svg';
import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { ListItem, VerticalList } from '~/ui/VerticalList';
import { GROWTHBOOK_FEATURES } from '~/utils/growthbook';

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

    // Extract module in URL or default to first module in list
    const selectedModule =
        searchParams.get('module') &&
        modulenames.includes(searchParams.get('module')!)
            ? searchParams.get('module')!
            : modulenames[0];

    // If module in URL exists but is not in module list, then delete module from URL
    useEffect(() => {
        if (
            searchParams.get('module') &&
            !modulenames.includes(searchParams.get('module')!)
        ) {
            const newSearchParams = new URLSearchParams(searchParams);
            newSearchParams.delete('module');
            setSearchParams(newSearchParams, { replace: true });
        }
    }, [searchParams, setSearchParams, modulenames]);

    const filteredModules =
        query === ''
            ? modulenames
            : modules
                  .filter(([name]) =>
                      name.toLowerCase().includes(query.toLowerCase())
                  )
                  .map(([name]) => name);

    const submitSearch = useCallback(() => {
        if (filteredModules.length === 1) {
            const convertedSearchParams = new URLSearchParams(searchParams);
            convertedSearchParams.set('module', filteredModules[0]);
            setSearchParams(convertedSearchParams);
        }
    }, [filteredModules, setSearchParams, searchParams]);

    const onChangeModule = (newModule: string) => {
        const convertedSearchParams = new URLSearchParams(searchParams);
        convertedSearchParams.set('module', newModule);
        setSearchParams(convertedSearchParams);
    };

    const isModuleFnExecEnabled = useFeature(
        GROWTHBOOK_FEATURES.MODULE_VIEW_INVOKE_FUNCTIONS
    ).on;

    return (
        <div className="flex flex-col md:flex-row md:flex-nowrap gap-5 border-0 border-y border-solid border-gray-45">
            <div className="w-full md:w-1/5">
                <Combobox value={selectedModule} onChange={onChangeModule}>
                    <div className="box-border border border-gray-50 border-solid rounded-md shadow-sm placeholder-gray-65 pl-3 w-full flex mt-2.5 justify-between py-1">
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
                            <SearchIcon className="fill-steel cursor-pointer h-4.5 w-4.5 align-middle" />
                        </button>
                    </div>
                    <Combobox.Options className="overflow-auto absolute left-0 w-full box-border md:w-1/6 md:left-auto h-fit max-h-verticalListLong overflow-auto shadow-moduleOption rounded-md bg-white z-10 px-2 pb-5 pt-3 flex flex-col gap-1">
                        {filteredModules.length > 0 ? (
                            <div className="text-caption font-semibold ml-1.5 pb-2 uppercase text-gray-75">
                                {filteredModules.length}
                                {filteredModules.length === 1
                                    ? ' Result'
                                    : ' Results'}
                            </div>
                        ) : (
                            <div className="text-gray-70 text-body italic pt-2 px-3.5 text-center">
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
                                                ? 'bg-sui/10 text-gray-80 border-transparent'
                                                : 'bg-white text-gray-80 font-medium border-transparent'
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
                                    onClick={() => onChangeModule(name)}
                                >
                                    {name}
                                </ListItem>
                            </div>
                        ))}
                    </VerticalList>
                </div>
            </div>
            <div
                className={clsx(
                    'border-0 md:border-l border-solid border-gray-45 md:pl-7 pt-5 grow overflow-auto',
                    isModuleFnExecEnabled && 'md:w-2/5'
                )}
            >
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
            {isModuleFnExecEnabled ? (
                <div className="border-0 md:border-l border-solid border-gray-45 md:pl-7 pt-5 grow overflow-auto md:w-3/5">
                    <TabGroup size="md">
                        <TabList>
                            <Tab>Execute</Tab>
                        </TabList>
                        <TabPanels>
                            <TabPanel>
                                <div className="overflow-auto h-verticalListLong">
                                    {id && selectedModule ? (
                                        <ModuleFunctionsInteraction
                                            packageId={id}
                                            moduleName={selectedModule}
                                        />
                                    ) : null}
                                </div>
                            </TabPanel>
                        </TabPanels>
                    </TabGroup>
                </div>
            ) : null}
        </div>
    );
}
export default PkgModuleViewWrapper;
