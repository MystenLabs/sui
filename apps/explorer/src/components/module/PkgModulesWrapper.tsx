// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect, useCallback } from 'react';
import { useSearchParams } from 'react-router-dom';

import ModuleView from './ModuleView';

import styles from './ModuleView.module.css';

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
                {data.content.map(([name], idx) => (
                    <button
                        onClick={clickModuleName(name)}
                        key={idx}
                        className={
                            idx === modulesPageNumber ? styles.activemodule : ''
                        }
                    >
                        {name}
                    </button>
                ))}
            </div>
            <div className={styles.modulewraper}>
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
            </div>
        </div>
    );
}
export default PkgModuleViewWrapper;
