// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '@mysten/ui';

import { ObjectLink } from '~/ui/InternalLink';
import { TransactionBlockCard, TransactionBlockCardSection } from '~/ui/TransactionBlockCard';

import type { OwnedObjectRef } from '@mysten/sui.js/client';

export function UpgradedSystemPackages({ data }: { data: OwnedObjectRef[] }) {
	if (!data?.length) return null;

	return (
		<TransactionBlockCard title="Changes" size="sm" shadow>
			<TransactionBlockCardSection
				title={
					<Text variant="body/semibold" color="success-dark">
						Updated
					</Text>
				}
			>
				<div className="flex flex-col gap-2">
					{data.map((object) => {
						const { objectId } = object.reference;
						return (
							<div className="flex flex-wrap items-center justify-between" key={objectId}>
								<div className="flex items-center gap-0.5">
									<Text variant="pBody/medium" color="steel-dark">
										Package
									</Text>
								</div>

								<div className="flex items-center">
									<ObjectLink objectId={objectId} />
								</div>
							</div>
						);
					})}
				</div>
			</TransactionBlockCardSection>
		</TransactionBlockCard>
	);
}
