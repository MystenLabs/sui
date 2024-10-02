// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useAppSelector } from '_hooks';
import { getNavIsVisible } from '_redux/slices/app';
import { Activity32, Apps32, Nft132, Tokens32 } from '@mysten/icons';
import cl from 'clsx';
import { NavLink } from 'react-router-dom';

import { useActiveAccount } from '../../hooks/useActiveAccount';
import st from './Navigation.module.scss';

export function Navigation() {
	const isVisible = useAppSelector(getNavIsVisible);
	const activeAccount = useActiveAccount();
	const makeLinkCls = ({ isActive }: { isActive: boolean }) =>
		cl(st.link, { [st.active]: isActive, [st.disabled]: activeAccount?.isLocked });
	const makeLinkClsNoDisabled = ({ isActive }: { isActive: boolean }) =>
		cl(st.link, { [st.active]: isActive });
	return (
		<nav
			className={cl('border-b-0 rounded-tl-md rounded-tr-md shrink-0', st.container, {
				[st.hidden]: !isVisible,
			})}
		>
			<div id="sui-apps-filters" className="flex whitespace-nowrap w-full justify-center"></div>
			<div className={st.navMenu}>
				<NavLink
					data-testid="nav-tokens"
					to="./tokens"
					className={makeLinkClsNoDisabled}
					title="Home"
				>
					<Tokens32 className="w-8 h-8" />
					<span className={st.title}>Home</span>
				</NavLink>
				<NavLink
					to="./nfts"
					className={makeLinkCls}
					title="Assets"
					onClick={(e) => {
						if (activeAccount?.isLocked) {
							e.preventDefault();
						}
					}}
				>
					<Nft132 className="w-8 h-8" />
					<span className={st.title}>Assets</span>
				</NavLink>
				<NavLink
					to="./apps"
					className={makeLinkCls}
					title="Apps"
					onClick={(e) => {
						if (activeAccount?.isLocked) {
							e.preventDefault();
						}
					}}
				>
					<Apps32 className="w-8 h-8" />
					<span className={st.title}>Apps</span>
				</NavLink>
				<NavLink
					data-testid="nav-activity"
					to="./transactions"
					className={makeLinkCls}
					title="Transactions"
					onClick={(e) => {
						if (activeAccount?.isLocked) {
							e.preventDefault();
						}
					}}
				>
					<Activity32 className="w-8 h-8" />
					<span className={st.title}>Activity</span>
				</NavLink>
			</div>
		</nav>
	);
}
