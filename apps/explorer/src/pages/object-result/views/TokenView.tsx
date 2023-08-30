// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { type SuiObjectResponse } from '@mysten/sui.js/client';

import { DynamicFieldsCard } from '~/components/Object/DynamicFieldsCard';
import { ObjectFieldsCard } from '~/components/Object/ObjectFieldsCard';
import TransactionBlocksForAddress from '~/components/TransactionBlocksForAddress/TransactionBlocksForAddress';

export function TokenView({ data }: { data: SuiObjectResponse }) {
	const objectId = data.data?.objectId!;

	return (
		<div className="flex flex-col flex-nowrap gap-14">
			<ObjectFieldsCard id={objectId} />
			<DynamicFieldsCard id={objectId} />
			<TransactionBlocksForAddress address={objectId} isObject />
		</div>
	);
}
