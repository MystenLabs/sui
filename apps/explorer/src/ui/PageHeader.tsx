// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Flag16, Info12 } from '@mysten/icons';
import { Heading, Placeholder, Text } from '@mysten/ui';

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
	after?: React.ReactNode;
	error?: string;
	loading?: boolean;
}

const TYPE_TO_COPY: Partial<Record<PageHeaderType, string>> = {
	Transaction: 'Transaction Block',
};

const TYPE_TO_ICON: Record<PageHeaderType, typeof CallIcon | null> = {
	Transaction: null,
	Checkpoint: Flag16,
	Object: null,
	Package: CallIcon,
	Address: null,
};

export function PageHeader({
	title,
	subtitle,
	type,
	before,
	error,
	loading,
	after,
}: PageHeaderProps) {
	const Icon = TYPE_TO_ICON[type];

	return (
		<div data-testid="pageheader" className="group">
			<div className="flex w-full items-center gap-3 sm:gap-5">
				{before && (
					<div className="self-start sm:self-center">
						<div className="sm:min-w-16 flex h-10 w-10 min-w-10 items-center justify-center rounded-lg bg-white/60 sm:h-16 sm:w-16 sm:rounded-xl lg:h-18 lg:w-18 lg:min-w-18">
							{loading ? <Placeholder rounded="xl" width="100%" height="100%" /> : before}
						</div>
					</div>
				)}
				<div className="flex w-full flex-col items-start justify-between gap-4 md:flex-row md:items-center">
					<div>
						<div className="mb-1 flex items-center gap-2">
							{Icon && <Icon className="text-steel-dark" />}
							{loading ? (
								<Placeholder rounded="lg" width="140px" />
							) : (
								<Text variant="captionSmall/semibold" color="hero-dark">
									{type in TYPE_TO_COPY ? TYPE_TO_COPY[type] : type}
								</Text>
							)}
						</div>
						<div className="min-w-0 break-words break-all">
							{loading ? (
								<Placeholder rounded="lg" width="540px" height="20px" />
							) : (
								<>
									{title && (
										<div className="flex items-center">
											<Heading as="h3" variant="heading3/semibold" color="gray-90" mono>
												{title}
											</Heading>
											<div className="ml-2 h-4 w-4 self-start md:h-6 md:w-6">
												<CopyToClipboard size="lg" color="steel" copyText={title} />
											</div>
										</div>
									)}
								</>
							)}
						</div>
						{subtitle && (
							<div className="mt-2 break-words">
								{loading ? (
									<Placeholder rounded="lg" width="540px" height="20px" />
								) : (
									<Text variant="body/medium" color="gray-75">
										{subtitle}
									</Text>
								)}
							</div>
						)}
					</div>
					{error && (
						<div className="mt-2">
							<Banner variant="neutralWhite" icon={<Info12 className="text-issue-dark" />}>
								<Text variant="pBody/medium" color="issue-dark">
									{error}
								</Text>
							</Banner>
						</div>
					)}
					{after && <div className="sm:self-center md:ml-auto">{after}</div>}
				</div>
			</div>
		</div>
	);
}
