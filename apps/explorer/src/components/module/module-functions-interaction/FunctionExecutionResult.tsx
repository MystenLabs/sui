// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LinkGroup } from './LinkGroup';
import { Banner } from '~/ui/Banner';

import type { SuiTransactionBlockResponse, OwnedObjectRef } from '@mysten/sui.js/client';

function toObjectLink(object: OwnedObjectRef) {
	return {
		text: object.reference.objectId,
		to: `/object/${encodeURIComponent(object.reference.objectId)}`,
	};
}

type FunctionExecutionResultProps = {
	result: SuiTransactionBlockResponse | null;
	error: string | false;
	onClear: () => void;
};

export function FunctionExecutionResult({ error, result, onClear }: FunctionExecutionResultProps) {
	const adjError = error || (result && result.effects?.status.error) || null;
	const variant = adjError ? 'error' : 'message';
	return (
		<Banner icon={null} fullWidth variant={variant} spacing="lg" onDismiss={onClear}>
			<div className="space-y-4 text-bodySmall">
				<LinkGroup
					title="Digest"
					links={
						result
							? [
									{
										text: result.digest,
										to: `/txblock/${encodeURIComponent(result.digest)}`,
									},
							  ]
							: []
					}
				/>
				<LinkGroup
					title="Created"
					links={(result && result.effects?.created?.map(toObjectLink)) || []}
				/>
				<LinkGroup
					title="Updated"
					links={(result && result.effects?.mutated?.map(toObjectLink)) || []}
				/>
				<LinkGroup title="Transaction failed" text={adjError} />
			</div>
		</Banner>
	);
}
