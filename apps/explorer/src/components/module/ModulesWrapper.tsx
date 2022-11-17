// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState, useEffect } from 'react';
import { useSearchParams } from 'react-router-dom';

import Pagination from '../../components/pagination/Pagination';
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

const MODULES_PER_PAGE = 3;
// TODO: Include Pagination for now use viewMore and viewLess
function ModuleViewWrapper({ id, data }: Props) {
    const [searchParams] = useSearchParams();
    const [modulesPageNumber, setModulesPageNumber] = useState(1);
    const totalModulesCount = data.content.length;
    const numOfMudulesToShow = MODULES_PER_PAGE;

    useEffect(() => {
        if (searchParams.get('module')) {
            const moduleIndex = data.content.findIndex(([moduleName]) => {
                return moduleName === searchParams.get('module');
            });

            setModulesPageNumber(
                Math.floor(moduleIndex / MODULES_PER_PAGE) + 1
            );
        }
    }, [searchParams, data.content]);

    const stats = {
        stats_text: 'total modules',
        count: totalModulesCount,
    };

    return (
        <div className={styles.modulewraper}>
            <h3 className={styles.title}>{data.title}</h3>
            <div className={styles.module}>
                {data.content
                    .filter(
                        (_, index) =>
                            index >=
                                (modulesPageNumber - 1) * numOfMudulesToShow &&
                            index < modulesPageNumber * numOfMudulesToShow
                    )
                    .map(([name, code]) => (
                        <div key={name}>
                            <div className={styles.moduletitle}>{name}</div>
                            <div className={styles.pagmodule}>
                                <ModuleView id={id} name={name} code={code} />
                            </div>
                        </div>
                    ))}
            </div>
            {totalModulesCount > numOfMudulesToShow && (
                <Pagination
                    totalItems={totalModulesCount}
                    itemsPerPage={numOfMudulesToShow}
                    currentPage={modulesPageNumber}
                    onPagiChangeFn={setModulesPageNumber}
                    stats={stats}
                />
            )}
        </div>
    );
}
export default ModuleViewWrapper;
