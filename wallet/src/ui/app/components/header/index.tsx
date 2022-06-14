// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { NavLink } from 'react-router-dom';

import BsIcon from '_components/bs-icon';

import st from './Header.module.scss';

function makeLinkCls({ isActive }: { isActive: boolean }) {
    return cl(st.link, { [st.active]: isActive });
}

function Header() {
    return (
        <div className={st.container}>
            <NavLink to="./tokens" className={makeLinkCls} title="Tokens">
                <BsIcon className={st.icon} icon="coin" />
            </NavLink>
            <NavLink to="./nfts" className={makeLinkCls} title="NFTs">
                <BsIcon className={st.icon} icon="collection" />
            </NavLink>
            <NavLink
                to="./transactions"
                className={makeLinkCls}
                title="Transactions"
            >
                <BsIcon className={st.icon} icon="arrow-left-right" />
            </NavLink>
            <NavLink to="./settings" className={makeLinkCls} title="Settings">
                <BsIcon className={st.icon} icon="gear" />
            </NavLink>
        </div>
    );
}

export default memo(Header);
