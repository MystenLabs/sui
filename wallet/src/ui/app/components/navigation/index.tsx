// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { NavLink } from 'react-router-dom';

import Icon, { SuiIcons } from '_components/icon';
import { useAppSelector } from '_hooks';
import { getNavIsVisible, getNavFilterTag } from '_redux/slices/app';

import st from './Navigation.module.scss';

function makeLinkCls({ isActive }: { isActive: boolean }) {
    return cl(st.link, { [st.active]: isActive });
}

function activeTagsFilter({ isActive }: { isActive: boolean }) {
    return cl({ [st.active]: isActive }, st.filter);
}

export type NavigationProps = {
    className?: string;
};

function Navigation({ className }: NavigationProps) {
    const isVisible = useAppSelector(getNavIsVisible);

    // TODO - use different method for the filters
    const filterTags = useAppSelector(getNavFilterTag);
    return (
        <nav
            className={cl(st.container, className, { [st.hidden]: !isVisible })}
        >
            {filterTags.length ? (
                <div className={st.filterTags}>
                    {filterTags.map((tag, index) => (
                        <NavLink
                            key={tag.link}
                            to={`./${tag.link}`}
                            end
                            className={activeTagsFilter}
                            title={tag.name}
                        >
                            <span className={st.title}>{tag.name}</span>
                        </NavLink>
                    ))}
                </div>
            ) : null}

            <div className={st.navMenu}>
                <NavLink to="./tokens" className={makeLinkCls} title="Tokens">
                    <Icon className={st.icon} icon={SuiIcons.Tokens} />
                    <span className={st.title}>Coins</span>
                </NavLink>
                <NavLink to="./nfts" className={makeLinkCls} title="NFTs">
                    <Icon className={st.icon} icon={SuiIcons.Nfts} />
                    <span className={st.title}>NFTs</span>
                </NavLink>
                <NavLink to="./apps" className={makeLinkCls} title="Apps">
                    <Icon
                        className={cl(st.icon, st.appsIcon)}
                        icon={SuiIcons.Apps}
                    />
                    <span className={st.title}>Apps</span>
                </NavLink>
                <NavLink
                    to="./transactions"
                    className={makeLinkCls}
                    title="Transactions"
                >
                    <Icon
                        className={cl(st.icon, st.walletActivityIcon)}
                        icon={SuiIcons.Activity}
                    />
                    <span className={st.title}>Activity</span>
                </NavLink>
            </div>
        </nav>
    );
}

export default memo(Navigation);
