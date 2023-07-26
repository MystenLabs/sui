// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LoadingIndicator } from '@mysten/ui';
import { useMemo } from 'react';

import { ModuleFunction } from './ModuleFunction';
import { useNormalizedMoveModule } from '~/hooks/useNormalizedMoveModule';
import { Banner } from '~/ui/Banner';

export type ModuleFunctionsInteractionProps = {
	packageId: string;
	moduleName: string;
};

export function ModuleFunctionsInteraction({
	packageId,
	moduleName,
}: ModuleFunctionsInteractionProps) {
	const {
		data: normalizedModule,
		error,
		isLoading,
	} = useNormalizedMoveModule(packageId, moduleName);
	const executableFunctions = useMemo(() => {
		if (!normalizedModule) {
			return [];
		}
		return Object.entries(normalizedModule.exposedFunctions)
			.filter(([_, anFn]) => anFn.isEntry)
			.map(([fnName, details]) => ({ name: fnName, details }));
	}, [normalizedModule]);
	const isEmpty = !isLoading && !executableFunctions.length && !error;
	if (isEmpty || error || isLoading) {
		return (
			<div className="flex h-full items-center justify-center">
				{error ? (
					<Banner variant="error">
						Error loading module <strong>{moduleName}</strong> details.
					</Banner>
				) : isEmpty ? (
					<div className="text-body font-medium text-steel-dark">
						No public entry functions found.
					</div>
				) : (
					<LoadingIndicator text="Loading data" />
				)}
			</div>
		);
	}
	return (
		<div className="flex flex-col gap-3">
			{executableFunctions.map(({ name, details }) => (
				<ModuleFunction
					key={name}
					functionName={name}
					functionDetails={details}
					moduleName={moduleName}
					packageId={packageId}
				/>
			))}
		</div>
	);
}
