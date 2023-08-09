// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { AccountAddress } from './AccountAddress';
import { DAppPermissionsList } from './DAppPermissionsList';
import { SummaryCard } from './SummaryCard';
import { Link } from '../shared/Link';
import { Heading } from '../shared/heading';
import { Text } from '../shared/text';
import { type PermissionType } from '_src/shared/messaging/messages/payloads/permissions';
import { getValidDAppUrl } from '_src/shared/utils';

export type DAppInfoCardProps = {
	name: string;
	url: string;
	iconUrl?: string;
	connectedAddress?: string;
	permissions?: PermissionType[];
};

export function DAppInfoCard({
	name,
	url,
	iconUrl,
	connectedAddress,
	permissions,
}: DAppInfoCardProps) {
	const validDAppUrl = getValidDAppUrl(url);
	const appHostname = validDAppUrl?.hostname ?? url;

	return (
		<div className="bg-white p-6 flex flex-1 flex-col gap-5">
			<div className="flex flex-row flex-nowrap items-center gap-3.75 py-3">
				<div className="flex items-stretch h-15 w-15 overflow-hidden bg-steel/20 shrink-0 grow-0 rounded-2xl">
					{iconUrl ? <img className="flex-1" src={iconUrl} alt={name} /> : null}
				</div>
				<div className="flex flex-col justify-start flex-nowrap gap-1">
					<Heading variant="heading4" weight="semibold" color="gray-100">
						{name}
					</Heading>
					<div className="self-start">
						<Link
							href={validDAppUrl?.toString() ?? url}
							title={name}
							text={appHostname}
							color="heroDark"
							weight="medium"
						/>
					</div>
				</div>
			</div>
			{connectedAddress ? (
				<div className="flex flex-nowrap flex-row items-center pt-3 gap-3">
					<Text variant="bodySmall" weight="medium" color="steel-darker" truncate>
						Connected account
					</Text>
					<div className="flex-1" />
					<AccountAddress copyable address={connectedAddress} />
				</div>
			) : null}
			<SummaryCard
				header="Permissions requested"
				body={permissions ? <DAppPermissionsList permissions={permissions} /> : null}
				boxShadow
			/>
		</div>
	);
}
