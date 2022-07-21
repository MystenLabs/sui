// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMemo, useState, useEffect } from 'react';

import Pagination from '../../components/pagination/Pagination';
import TxModuleView from './TxModuleView';

import styles from './TxModuleView.module.css';

type TxModules = {
    title: string;
    content: any[];
};

const TX_MODULES_PER_PAGE = 3;
// TODO: Include Pagination for now use viewMore and viewLess
function TxModuleViewWrapper({ data }: { data: TxModules }) {
    const moduleData = useMemo(() => data, [data]);
    const [modulesPageNumber, setModulesPageNumber] = useState(1);
    const totalModulesCount = moduleData.content.length;
    const numOfMudulesToShow = TX_MODULES_PER_PAGE;

    useEffect(() => {
        setModulesPageNumber(modulesPageNumber);
    }, [modulesPageNumber]);

    const stats = {
        stats_text: 'total modules',
        count: totalModulesCount,
    };

    return (
        <div className={styles.modulewraper}>
            <h3 className={styles.txtitle}>Modules </h3>
            <div className={styles.txmodule}>
                {moduleData.content
                    .filter(
                        (_, index) =>
                            index >=
                                (modulesPageNumber - 1) * numOfMudulesToShow &&
                            index < modulesPageNumber * numOfMudulesToShow
                    )
                    .map((item, idx) => (
                        <TxModuleView itm={item} key={idx} />
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
export default TxModuleViewWrapper;
