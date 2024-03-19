// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Tabs, TabsContent, TabsList, TabsTrigger } from '~/ui/Tabs';
import { Text } from '@mysten/ui';

import ModuleView from './ModuleView';
import { useVerifiedSourceCode } from '~/hooks/useVerifiedSourceCode';
import clsx from 'clsx';

type ModuleCodeTabsProps = {
	packageId: string;
	moduleName: string;
	moduleBytecode: string;
	isCompact: boolean;
};

export function ModuleCodeTabs({
	packageId,
	moduleName,
	moduleBytecode,
	isCompact,
}: ModuleCodeTabsProps) {
	const { data: verifiedSourceCode } = useVerifiedSourceCode({
		packageId,
		moduleName,
	});

	return (
		<Tabs defaultValue="bytecode" className="h-full grow overflow-auto border-gray-45 pt-5 md:pl-7">
			<TabsList>
				<TabsTrigger className="h-6" value="bytecode">
					Bytecode
				</TabsTrigger>
				{verifiedSourceCode ? (
					<TabsTrigger className="h-6" value="source">
						Source
						<div className="rounded bg-success-light px-1.5 py-1">
							<Text variant="subtitle/medium" color="success-dark">
								Verified
							</Text>
						</div>
					</TabsTrigger>
				) : null}
			</TabsList>
			<TabsContent value="bytecode">
				<div className={clsx('overflow-auto', { 'h-verticalListLong': isCompact })}>
					<ModuleView id={packageId} name={moduleName} code={moduleBytecode} />
				</div>
			</TabsContent>
			{verifiedSourceCode ? (
				<TabsContent value="source">
					<div className={clsx('overflow-auto', { 'h-verticalListLong': isCompact })}>
						<ModuleView id={packageId} name={moduleName} code={verifiedSourceCode} />
					</div>
				</TabsContent>
			) : null}
		</Tabs>
	);
}
