// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';
import { memo } from 'react';
import { Link } from 'react-router-dom';

import type { ReactNode, ButtonHTMLAttributes, AnchorHTMLAttributes } from 'react';

import st from './Button.module.scss';

export type ButtonProps = {
	className?: string;
	mode?: 'neutral' | 'primary' | 'outline' | 'secondary';
	size?: 'mini' | 'small' | 'large';
	children: ReactNode | ReactNode[];
	disabled?: boolean;
	title?: string;
} & (
	| {
			href: string;
			onClick?: AnchorHTMLAttributes<HTMLAnchorElement>['onClick'];
	  }
	| {
			onClick?: ButtonHTMLAttributes<HTMLButtonElement>['onClick'];
			type?: ButtonHTMLAttributes<HTMLButtonElement>['type'];
	  }
);

function Button(props: ButtonProps) {
	const { className, mode = 'neutral', size = 'large', children, disabled = false, title } = props;
	const commonProps = {
		className: cl(st.container, className, st[mode], st[size], {
			[st.disabled]: disabled,
		}),
		disabled,
		children,
		title,
		tabIndex: disabled ? -1 : undefined,
	};
	if (!disabled && 'href' in props) {
		return <Link to={props.href} {...commonProps} onClick={props.onClick} />;
	}
	let type: ButtonHTMLAttributes<HTMLButtonElement>['type'] = 'button';
	let onClick = undefined;
	if (!('href' in props)) {
		if (props.type) {
			type = props.type;
		}
		onClick = props.onClick;
	}
	return <button {...commonProps} {...{ type, onClick }} />;
}

export default memo(Button);
