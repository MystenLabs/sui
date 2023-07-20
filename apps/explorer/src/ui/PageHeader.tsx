// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Flag16, Nft16 } from '@mysten/icons';
import { Heading } from '@mysten/ui';

import { Badge } from './Badge';
import { ReactComponent as SenderIcon } from './icons/sender.svg';
import { ReactComponent as CallIcon } from './icons/transactions/call.svg';
import { CopyToClipboard } from '~/ui/CopyToClipboard';

export type PageHeaderType = 'Transaction' | 'Checkpoint' | 'Address' | 'Object' | 'Package';

export interface PageHeaderProps {
	title: string;
	subtitle?: string | null;
	type: PageHeaderType;
	status?: 'success' | 'failure';
}

const TYPE_TO_COPY: Partial<Record<PageHeaderType, string>> = {
	Transaction: 'Transaction Block',
};

const TYPE_TO_ICON: Record<PageHeaderType, typeof CallIcon> = {
	Transaction: CallIcon,
	Checkpoint: Flag16,
	Object: Nft16,
	Package: CallIcon,
	Address: () => (
		<SenderIcon
			style={{
				'--icon-primary-color': 'var(--steel)',
				'--icon-secondary-color': 'white',
			}}
		/>
	),
};

const STATUS_TO_TEXT = {
	success: 'Success',
	failure: 'Failure',
};

export function PageHeader({ title, subtitle, type, status }: PageHeaderProps) {
	const Icon = TYPE_TO_ICON[type];

	return (
		<div data-testid="pageheader">
			<div className="mb-3 flex items-center gap-2">
				{Icon && <Icon className="text-steel-dark" />}
				<Heading variant="heading4/semibold" color="steel-darker">
					{type in TYPE_TO_COPY ? TYPE_TO_COPY[type] : type}
				</Heading>
			</div>
			<div className="flex flex-col gap-2 lg:flex-row">
				<div className="flex min-w-0 items-center gap-2">
					<div className="min-w-0 break-words">
						<Heading as="h2" variant="heading2/semibold" color="gray-90" mono>
							{title}
						</Heading>
					</div>
					<CopyToClipboard size="lg" color="steel" copyText={title} />
				</div>

				{status && (
					<div>
						<Badge variant={status}>{STATUS_TO_TEXT[status]}</Badge>
					</div>
				)}
			</div>
			{subtitle && (
				<div className="mt-2 break-words">
					<Heading variant="heading4/semibold" color="gray-75">
						{subtitle}
					</Heading>
				</div>
			)}
		</div>
	);
}
