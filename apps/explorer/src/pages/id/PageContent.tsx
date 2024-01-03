// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Divider } from '~/ui/Divider';
import { FieldsContent } from '~/pages/object-result/views/TokenView';
import { Modules } from '~/pages/id/Modules';
import { translate } from '~/pages/object-result/ObjectResultType';
import { OwnedObjectsSection } from '~/pages/id/OwnedObjectsSection';
import { TransactionBlocksTable } from '~/pages/id/TransactionBlocksTable';
import { useGetObject } from '@mysten/core';

export const PACKAGE_TYPE_NAME = 'Move Package';

export function PageContent({ address }: { address: string }) {
	const { data } = useGetObject(address);
	const isObject = !!data?.data;
	const resp = data && isObject ? translate(data) : null;
	const isPackage = resp ? resp.objType === PACKAGE_TYPE_NAME : false;

	return (
		<div>
			<section>
				<OwnedObjectsSection address={address} />
			</section>

			<Divider />

			{isObject && !isPackage && (
				<section className="mt-14">
					<FieldsContent objectId={address} />
				</section>
			)}

			{isPackage && resp && (
				<section className="mt-14">
					<Modules data={resp} />
				</section>
			)}

			<section className="mt-14">
				<TransactionBlocksTable address={address} isObject={isObject} />
			</section>
		</div>
	);
}
