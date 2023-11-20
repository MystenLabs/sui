// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useMenuIsOpen, useNextMenuUrl } from '_components/menu/hooks';
import cl from 'clsx';
import { memo } from 'react';
import { Link } from 'react-router-dom';

import st from './MenuButton.module.scss';

export type MenuButtonProps = {
	className?: string;
};

function MenuButton({ className }: MenuButtonProps) {
	const isOpen = useMenuIsOpen();
	const menuUrl = useNextMenuUrl(!isOpen, '/');
	return (
		<Link
			data-testid="menu"
			className={cl(st.button, { [st.open]: isOpen }, className)}
			to={menuUrl}
		>
			<span className={cl(st.line, st.line1)} />
			<span className={cl(st.line, st.line2)} />
			<span className={cl(st.line, st.line3)} />
		</Link>
	);
}

export default memo(MenuButton);
