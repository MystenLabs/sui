// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Divider } from '~/ui/Divider';
import { FieldsContent } from '~/pages/object-result/views/TokenView';
import { Modules } from '~/pages/id/Modules';
import { type DataType } from '~/pages/object-result/ObjectResultType';
import { OwnedObjectsSection } from '~/pages/id/OwnedObjectsSection';
import { TransactionBlocksTable } from '~/pages/id/TransactionBlocksTable';

export function PageContent({
	address,
	pageType,
	data,
}: {
	address: string;
	pageType: 'Package' | 'Object' | 'Address';
	data?: DataType | null;
}) {
	return (
		<div>
			<section>
				<OwnedObjectsSection address={address} />
			</section>

			<Divider />

			{pageType === 'Object' && (
				<section className="mt-14">
					<FieldsContent objectId={address} />
				</section>
			)}

			{pageType === 'Package' && data && (
				<section className="mt-14">
					<Modules data={data} />
				</section>
			)}

			<section className="mt-14">
				<TransactionBlocksTable address={address} pageType={pageType} />
			</section>
		</div>
	);
}
