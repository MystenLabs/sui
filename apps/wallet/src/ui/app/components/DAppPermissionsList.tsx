// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { PermissionType } from '_messages/payloads/permissions';
import { CheckFill12 } from '@mysten/icons';

import { Text } from '../shared/text';

export type DAppPermissionsListProps = {
	permissions: PermissionType[];
};

const permissionTypeToTxt: Record<PermissionType, string> = {
	viewAccount: 'Share wallet address',
	suggestTransactions: 'Suggest transactions to approve',
};

export function DAppPermissionsList({ permissions }: DAppPermissionsListProps) {
	return (
		<ul className="list-none m-0 p-0 flex flex-col gap-3">
			{permissions.map((aPermission) => (
				<li key={aPermission} className="flex flex-row flex-nowrap items-center gap-2">
					<CheckFill12 className="text-steel" />
					<Text variant="bodySmall" weight="medium" color="steel-darker">
						{permissionTypeToTxt[aPermission]}
					</Text>
				</li>
			))}
		</ul>
	);
}
