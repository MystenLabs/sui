// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowUpRight16 } from '@mysten/icons';

import { formatAddress } from '@mysten/sui.js/utils';
import { ExplorerLinkType } from './ExplorerLinkType';
import { type ExplorerLinkConfig, useExplorerLink } from '../../hooks/useExplorerLink';
import { Text } from '../../shared/text';
import ExternalLink from '_components/external-link';

import type { ReactNode } from 'react';

import st from './ExplorerLink.module.scss';

export type ExplorerLinkProps = ExplorerLinkConfig & {
	track?: boolean;
	children?: ReactNode;
	className?: string;
	title?: string;
	showIcon?: boolean;
};

function ExplorerLink({
	track,
	children,
	className,
	title,
	showIcon,
	...linkConfig
}: ExplorerLinkProps) {
	const explorerHref = useExplorerLink(linkConfig);
	if (!explorerHref) {
		return null;
	}

	return (
		<ExternalLink href={explorerHref} className={className} title={title}>
			<>
				{children} {showIcon && <ArrowUpRight16 className={st.explorerIcon} />}
			</>
		</ExternalLink>
	);
}

export function AddressLink({ address }: { address: string }) {
	return (
		<ExplorerLink
			type={ExplorerLinkType.address}
			address={address}
			className="text-hero-dark no-underline inline-block"
		>
			<Text variant="subtitle" weight="semibold" truncate mono>
				{formatAddress(address)}
			</Text>
		</ExplorerLink>
	);
}

export default ExplorerLink;
