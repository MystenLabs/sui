// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import codestyle from '../../styles/bytecode.module.css';

import styles from './TxModuleView.module.css';

function TxModuleView({ itm }: { itm: any }) {
    return (
        <section>
            <div className={styles.moduletitle}>{itm[0]}</div>
            <div className={cl(codestyle.code, styles.codeview)}>{itm[1]}</div>
        </section>
    );
}

export default TxModuleView;
