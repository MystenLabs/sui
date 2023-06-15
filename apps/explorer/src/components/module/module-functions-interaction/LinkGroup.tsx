// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Link } from '~/ui/Link';

export type LinkGroupProps = {
	title: string;
} & ({ text: string | null } | { links: { text: string; to: string }[] });

export function LinkGroup(props: LinkGroupProps) {
	const { title } = props;
	const isLinks = 'links' in props;
	const isText = 'text' in props;
	if ((isLinks && !props.links.length) || (isText && !props.text)) {
		return null;
	}
	return (
		<div className="space-y-3">
			<div className="font-semibold text-gray-90">{title}</div>
			{isLinks
				? props.links.map(({ text, to }) => (
						<div key={to}>
							<Link to={to} variant="mono">
								{text}
							</Link>
						</div>
				  ))
				: null}
			{isText ? <div className="text-pBodySmall font-medium text-gray-90">{props.text}</div> : null}
		</div>
	);
}
