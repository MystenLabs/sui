// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';

import ModuleView from './ModuleView';

import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { ListItem, VerticalList } from '~/ui/VerticalList';

interface Props {
    id?: string;
    modules: [moduleName: string, code: string][];
}

function PkgModuleViewWrapper({ id, modules }: Props) {
    const [searchParams, setSearchParams] = useSearchParams();
    const [modulesPageNumber, setModulesPageNumber] = useState(0);

    const clickModuleName = useCallback(
        (module: string) => () => {
            const moduleIndex = modules.findIndex(
                ([moduleName]) => moduleName === module
            );

            setSearchParams({ module });

            setModulesPageNumber(moduleIndex);
        },
        [modules, setSearchParams]
    );

    useEffect(() => {
        if (searchParams.get('module')) {
            const moduleIndex = modules.findIndex(([moduleName]) => {
                return moduleName === searchParams.get('module');
            });

            setModulesPageNumber(moduleIndex);
        }
    }, [searchParams, modules]);

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
                <VerticalList>
                    {modules.map(([name], idx) => (
                        <div
                            key={idx}
                            className="w-full lg:min-w-[12vw] lg:w-fit"
                        >
                            <ListItem
                                active={idx === modulesPageNumber}
                                onClick={clickModuleName(name)}
                            >
                                {name}
                            </ListItem>
                        </div>
                    ))}
                </VerticalList>
            </div>
            <div className="border-0 lg:border-l border-solid border-sui-grey-45 lg:pl-[30px] pt-[20px]">
                <TabGroup size="md">
                    <TabList>
                        <Tab>Bytecode</Tab>
                    </TabList>
                    <TabPanels>
                        <TabPanel>
                            <div className="overflow-auto h-[555px] w-[87vw] lg:w-[75vw]">
                                {[modules[modulesPageNumber]].map(
                                    ([name, code], idx) => (
                                        <ModuleView
                                            key={idx}
                                            id={id}
                                            name={name}
                                            code={code}
                                        />
                                    )
                                )}
                            </div>
                        </TabPanel>
                    </TabPanels>
                </TabGroup>
            </div>
        </div>
    );
}
export default PkgModuleViewWrapper;
