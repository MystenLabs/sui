// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Flag16, Info12, Nft16 } from '@mysten/icons';
import { Heading, Text } from '@mysten/ui';

import { ReactComponent as CallIcon } from './icons/transactions/call.svg';
import { Banner } from '~/ui/Banner';
import { CopyToClipboard } from '~/ui/CopyToClipboard';

export type PageHeaderType = 'Transaction' | 'Checkpoint' | 'Address' | 'Object' | 'Package';

export interface PageHeaderProps {
	title: string;
	subtitle?: string | null;
	type: PageHeaderType;
	status?: 'success' | 'failure';
	before?: React.ReactNode;
	error?: string;
}

const TYPE_TO_COPY: Partial<Record<PageHeaderType, string>> = {
	Transaction: 'Transaction Block',
};

const TYPE_TO_ICON: Record<PageHeaderType, typeof CallIcon | null> = {
	Transaction: null,
	Checkpoint: Flag16,
	Object: Nft16,
	Package: CallIcon,
	Address: null,
};

export function PageHeader({ title, subtitle, type, before, error }: PageHeaderProps) {
	const Icon = TYPE_TO_ICON[type];

	return (
		<div data-testid="pageheader" className="group">
			<div className="flex items-center gap-5">
				{before}
				<div>
					<div className="mb-1 flex items-center gap-2">
						{Icon && <Icon className="text-steel-dark" />}
						<Heading variant="heading4/semibold" color="steel-darker">
							{type in TYPE_TO_COPY ? TYPE_TO_COPY[type] : type}
						</Heading>
					</div>
					<div className="flex flex-col gap-2">
						<div className="flex flex-col gap-2 lg:flex-row">
							<div className="flex min-w-0 items-center gap-2">
								<div className="min-w-0 break-words break-all">
									<Heading as="h2" variant="heading2/semibold" color="gray-90" mono>
										{title}
									</Heading>
								</div>
								<div className="flex items-center group-hover/gradientContent:!flex group-hover:!flex md:hidden">
									<CopyToClipboard size="lg" color="steel" copyText={title} />
								</div>
							</div>
						</div>
						{error && (
							<div>
								<Banner variant="neutralWhite" icon={<Info12 className="text-issue-dark" />}>
									<Text variant="pBody/medium" color="issue-dark">
										{error}
									</Text>
								</Banner>
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
			</div>
		</div>
	);
}
