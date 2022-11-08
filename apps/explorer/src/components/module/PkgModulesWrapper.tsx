// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Combobox } from '@headlessui/react';
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

const initialSelectModule = (searchParams: any, modulenames: string[]) => {
    const paramModule = searchParams.get('module') || modulenames?.[0] || null;

    if (!!paramModule && modulenames.includes(paramModule)) {
        return paramModule;
    } else {
        return modulenames[0];
    }
};

function ModuleViewWrapper({
    id,
    selectedModuleName,
    modules,
}: ModuleViewWrapperProps) {
    const selectedModuleData = modules.find(
        ([name]) => name === selectedModuleName
    );

    if (selectedModuleData) {
        const [name, code] = selectedModuleData;

        return <ModuleView id={id} name={name} code={code} />;
    }

    return <div />;
}

function PkgModuleViewWrapper({ id, modules }: Props) {
    const modulenames = modules.map(([name], idx) => name);
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

    const submitSearch = useCallback(() => {
        setSelectedModule((prev: string) =>
            filteredModules.length === 1 ? filteredModules[0] : prev
        );
    }, [filteredModules]);

    return (
        <div className="flex flex-col md:flex-row md:flex-nowrap gap-5 border-0 border-y border-solid border-sui-grey-45">
            <div className="w-full md:w-1/5">
                <Combobox value={selectedModule} onChange={setSelectedModule}>
                    <div className="box-border border border-sui-grey-50 border-solid rounded-md shadow-sm placeholder-sui-grey-65 pl-3 w-full flex mt-2.5 justify-between py-2">
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
                            <SearchIcon className="fill-sui-steel cursor-pointer" />
                        </button>
                    </div>
                    <Combobox.Options
                        as="div"
                        className=" overflow-auto pr-1 absolute w-full md:w-1/6 h-fit pb-0.5 shadow-moduleOption rounded-md bg-white"
                    >
                        <VerticalList>
                            {filteredModules.length > 0 ? (
                                <div className="text-caption font-semibold ml-0.5 px-3 py-2 uppercase text-sui-grey-75">
                                    {filteredModules.length} Result
                                    {filteredModules.length === 1 ? '' : 's'}
                                </div>
                            ) : (
                                <div className="text-sui-grey-70 text-body italic py-5 px-3.5 text-center">
                                    No results
                                </div>
                            )}
                            {filteredModules.map((name, idx) => (
                                <Combobox.Option key={name} value={name}>
                                    {({ active }) => (
                                        <div key={idx} className="md:min-w-fit">
                                            <ListItem active={active}>
                                                {name}
                                            </ListItem>
                                        </div>
                                    )}
                                </Combobox.Option>
                            ))}
                        </VerticalList>
                    </Combobox.Options>
                </Combobox>
                <div className="h-verticalListShort md:h-verticalListLong overflow-auto">
                    <VerticalList>
                        {modulenames.map((name, idx) => (
                            <div key={idx} className="md:min-w-fit">
                                <ListItem active={selectedModule === name}>
                                    {name}
                                </ListItem>
                            </div>
                        ))}
                    </VerticalList>
                </div>
            </div>
            <div className="border-0 md:border-l border-solid border-sui-grey-45 md:pl-7 pt-5 grow">
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
