// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { TabHeader } from '~/ui/Tabs';
import { RadioGroup, RadioGroupItem } from '@mysten/ui';
import { ErrorBoundary } from '~/components/error-boundary/ErrorBoundary';
import PkgModulesWrapper from '~/components/module/PkgModulesWrapper';
import { useState } from 'react';
import { type Direction } from 'react-resizable-panels';
import { type DataType } from '~/pages/object-result/ObjectResultType';
import { checkIsPropertyType } from '~/utils/objectUtils';

const splitPanelsOrientation: { label: string; value: Direction }[] = [
	{ label: 'STACKED', value: 'vertical' },
	{ label: 'SIDE-BY-SIDE', value: 'horizontal' },
];

export function Modules({ data }: { data: DataType }) {
	const [selectedSplitPanelOrientation, setSplitPanelOrientation] = useState(
		splitPanelsOrientation[1].value,
	);

	const properties = Object.entries(data.data?.contents)
		.filter(([key, _]) => key !== 'name')
		.filter(([_, value]) => checkIsPropertyType(value));

	return (
		<TabHeader
			title="Modules"
			after={
				<div className="hidden md:block">
					<RadioGroup
						aria-label="split-panel-bytecode-viewer"
						value={selectedSplitPanelOrientation}
						onValueChange={(value) => setSplitPanelOrientation(value as 'vertical' | 'horizontal')}
					>
						{splitPanelsOrientation.map(({ value, label }) => (
							<RadioGroupItem key={value} value={value} label={label} />
						))}
					</RadioGroup>
				</div>
			}
		>
			<ErrorBoundary>
				<PkgModulesWrapper
					id={data.id}
					modules={properties}
					splitPanelOrientation={selectedSplitPanelOrientation}
				/>
			</ErrorBoundary>
		</TabHeader>
	);
}
