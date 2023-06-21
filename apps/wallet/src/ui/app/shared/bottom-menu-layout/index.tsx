// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { Children, memo, useEffect, useRef, useState } from 'react';

import type { ReactNode } from 'react';

import st from './BottomMenuLayout.module.scss';

export type BottomMenuLayoutProps = {
	className?: string;
	children: [ReactNode, ReactNode];
};

function BottomMenuLayout({ className, children }: BottomMenuLayoutProps) {
	if (Children.count(children) < 2) {
		//eslint-disable-next-line no-console
		console.warn(
			'[BottomMenuLayout] expects 2 children. First child should be the content and the second the bottom menu',
		);
	}
	return <div className={cl(st.container, className)}>{children}</div>;
}

type ContentMenuProps = {
	className?: string;
	children: ReactNode | ReactNode[];
};

function ContentNoMemo({ className, children }: ContentMenuProps) {
	return <div className={cl(className, st.content)}>{children}</div>;
}

function MenuNoMemo({
	className,
	children,
	stuckClass,
}: ContentMenuProps & { stuckClass: string }) {
	const [isStuck, setIsStuck] = useState(false);
	const menuRef = useRef(null);
	useEffect(() => {
		if (menuRef.current) {
			const observer = new IntersectionObserver(
				([entry]) => {
					setIsStuck(entry.intersectionRatio < 1);
				},
				{ threshold: 1 },
			);
			observer.observe(menuRef.current);
			return () => {
				observer.disconnect();
			};
		}
	});
	return (
		<div className={cl(className, st.menu, { [stuckClass]: isStuck })} ref={menuRef}>
			{children}
		</div>
	);
}

export default memo(BottomMenuLayout);
export const Content = memo(ContentNoMemo);
export const Menu = memo(MenuNoMemo);
