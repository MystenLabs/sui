// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { Link } from 'react-router-dom';

import st from './Select.module.scss';

const selections = [
    {
        title: 'Yes, create a new account.',
        desc: 'This will create a new wallet and Recovery Phrase',
        url: '../create',
        action: 'Create new wallet',
    },
    {
        title: 'No, I already have a Recovery Phrase.',
        desc: 'Import an existing wallet using a Secret Recovery Phrase',
        url: '../import',
        action: 'Import a wallet',
    },
];

const SelectPage = () => {
    return (
        <>
            <h1>New to Sui Wallet?</h1>
            {selections.map((aSelection) => (
                <div className={st.card} key={aSelection.url}>
                    <h3 className={st.title}>{aSelection.title}</h3>
                    <div className={st.desc}>{aSelection.desc}</div>
                    <Link to={aSelection.url} className={cl('btn', st.action)}>
                        {aSelection.action}
                    </Link>
                </div>
            ))}
        </>
    );
};

export default SelectPage;
