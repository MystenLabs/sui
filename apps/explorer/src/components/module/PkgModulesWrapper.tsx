// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';

import ModuleView from './ModuleView';

import styles from './ModuleView.module.css';

import { Tab, TabGroup, TabList, TabPanel, TabPanels } from '~/ui/Tabs';
import { ListItem, VerticalList } from '~/ui/VerticalList';

type Modules = {
    title: string;
    content: [moduleName: string, code: string][];
};

interface Props {
    id?: string;
    data: Modules;
}

function PkgModuleViewWrapper({ id, data }: Props) {
    const [searchParams, setSearchParams] = useSearchParams();
    const [modulesPageNumber, setModulesPageNumber] = useState(0);

    const clickModuleName = useCallback(
        (module: string) => () => {
            const moduleIndex = data.content.findIndex(
                ([moduleName]) => moduleName === module
            );

            setSearchParams({ module });

            setModulesPageNumber(moduleIndex);
        },
        [data.content, setSearchParams]
    );

    useEffect(() => {
        if (searchParams.get('module')) {
            const moduleIndex = data.content.findIndex(([moduleName]) => {
                return moduleName === searchParams.get('module');
            });

            setModulesPageNumber(moduleIndex);
        }
    }, [searchParams, data.content]);

    return (
        <div className={styles.pkgmodulewrapper}>
            <div className={styles.modulelist}>
                <VerticalList>
                    {data.content.map(([name], idx) => (
                        <ListItem
                            key={idx}
                            active={idx === modulesPageNumber}
                            onClick={clickModuleName(name)}
                        >
                            {name}
                        </ListItem>
                    ))}
                </VerticalList>
            </div>
            <div
                className={`${styles.modulewraper} ${styles.singlemodulewrapper}`}
            >
                <TabGroup size="md">
                    <TabList>
                        <Tab>Bytecode</Tab>
                    </TabList>
                    <TabPanels>
                        <TabPanel>
                            <div className={styles.singlemodule}>
                                {[data.content[modulesPageNumber]].map(
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
