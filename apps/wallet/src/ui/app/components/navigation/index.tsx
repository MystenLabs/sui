// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Activity32, Apps32, Nft132, Tokens32 } from '@mysten/icons';
import cl from 'classnames';
import { memo } from 'react';
import { NavLink } from 'react-router-dom';

import { useActiveAccount } from '../../hooks/useActiveAccount';
import { useIsAccountReadLocked } from '../../hooks/useIsAccountReadLocked';
import { useAppSelector } from '_hooks';
import { getNavIsVisible } from '_redux/slices/app';

import st from './Navigation.module.scss';

export type NavigationProps = {
	className?: string;
};

function Navigation({ className }: NavigationProps) {
	const isVisible = useAppSelector(getNavIsVisible);
	const activeAccount = useActiveAccount();
	const isActiveAccountReadLocked = useIsAccountReadLocked(activeAccount);
	const makeLinkCls = ({ isActive }: { isActive: boolean }) =>
		cl(st.link, { [st.active]: isActive, [st.disabled]: isActiveAccountReadLocked });
	const makeLinkClsNoDisabled = ({ isActive }: { isActive: boolean }) =>
		cl(st.link, { [st.active]: isActive });
	return (
		<nav
			className={cl('border-b-0 rounded-tl-md rounded-tr-md pt-2 pb-0', st.container, className, {
				[st.hidden]: !isVisible,
			})}
		>
			<div
				id="sui-apps-filters"
				className="flex overflow-x:hidden whitespace-nowrap w-full justify-center"
			></div>

			<div className={st.navMenu}>
				<NavLink
					data-testid="nav-tokens"
					to="./tokens"
					className={makeLinkClsNoDisabled}
					title="Tokens"
				>
					<Tokens32 className="w-8 h-8" />
					<span className={st.title}>Coins</span>
				</NavLink>
				<NavLink
					to="./nfts"
					className={makeLinkCls}
					title="Assets"
					onClick={(e) => {
						if (isActiveAccountReadLocked) {
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
						if (isActiveAccountReadLocked) {
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
						if (isActiveAccountReadLocked) {
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

export default memo(Navigation);
