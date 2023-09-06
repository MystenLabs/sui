// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { LoadingIndicator, Text } from '@mysten/ui';
import React from 'react';

import { type SuiDependency, type SuiPackage } from '~/components/module/dependencyUtils';
import { DependenciesCard } from '~/pages/object-result/views/DependenciesCard';

interface Props {
	selectedModuleName: string;
	isLoading: boolean;
	suiPackage?: SuiPackage;
}

function DependencyView({ selectedModuleName, isLoading, suiPackage }: Props) {
	const selectedModule = suiPackage?.modules.find(
		(element) => element.module === selectedModuleName,
	);

	if (isLoading || !selectedModule?.dependencies) {
		return (
			<div className="flex h-full items-center justify-center">
				<LoadingIndicator />
				<Text color="steel" variant="body/medium">
					loading data
				</Text>
			</div>
		);
	}

	const dependencies: SuiDependency[] = selectedModule.dependencies;

	return (
		<>
			<div className="title mb-2 ml-2 mt-1 break-words font-medium">
				Module <b>{selectedModuleName}</b> has <b>{dependencies.length}</b>{' '}
				{dependencies.length === 1 ? <>dependency.</> : <>dependencies.</>}
			</div>
			<div data-testid="dependencies-card" className="flex items-stretch">
				<DependenciesCard suiDependencies={dependencies} defaultOpen />
			</div>
		</>
	);
}

export default DependencyView;
