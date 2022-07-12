// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { NavLink } from 'react-router-dom';

import Icon, { SuiIcons } from '_components/icon';

import st from './Navigation.module.scss';

function makeLinkCls({ isActive }: { isActive: boolean }) {
    return cl(st.link, { [st.active]: isActive });
}

export type NavigationProps = {
    className?: string;
};

function Navigation({ className }: NavigationProps) {
    return (
        <nav className={cl(st.container, className)}>
            <NavLink to="./tokens" className={makeLinkCls} title="Tokens">
                <Icon className={st.icon} icon={SuiIcons.Tokens} />
                <span className={st.title}>Coins</span>
            </NavLink>
            <NavLink to="./nfts" className={makeLinkCls} title="NFTs">
                <Icon className={st.icon} icon={SuiIcons.Nfts} />
                <span className={st.title}>NFTs</span>
            </NavLink>
            <NavLink
                to="./transactions"
                className={makeLinkCls}
                title="Transactions"
            >
                <Icon className={st.icon} icon={SuiIcons.History} />
                <span className={st.title}>Activity</span>
            </NavLink>
            <NavLink to="./settings" className={makeLinkCls} title="Settings">
                <Icon className={st.icon} icon={SuiIcons.Apps} />
                <span className={st.title}>Settings</span>
            </NavLink>
        </nav>
    );
}

export default memo(Navigation);
