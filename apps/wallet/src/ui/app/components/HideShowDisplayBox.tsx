// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Copy16, EyeClose16, EyeOpen16 } from '@mysten/icons';
import { cx } from 'class-variance-authority';
import { useEffect, useState } from 'react';

import { useCopyToClipboard } from '../hooks/useCopyToClipboard';
import { Link } from '../shared/Link';
import { Text } from '../shared/text';

const AUTO_HIDE_INTERVAL = 3 * 60 * 1000;

export type HideShowDisplayBoxProps = {
	value: string | string[];
	hideCopy?: boolean;
	copiedMessage?: string;
};

export function HideShowDisplayBox({
	value,
	hideCopy = false,
	copiedMessage,
}: HideShowDisplayBoxProps) {
	const [valueHidden, setValueHidden] = useState(true);
	const copyCallback = useCopyToClipboard(
		hideCopy ? '' : typeof value === 'string' ? value : value.join(' '),
		{
			copySuccessMessage: copiedMessage,
		},
	);
	useEffect(() => {
		const updateOnVisibilityChange = () => {
			if (document.visibilityState === 'hidden') {
				setValueHidden(true);
			}
		};
		document.addEventListener('visibilitychange', updateOnVisibilityChange);
		return () => {
			document.removeEventListener('visibilitychange', updateOnVisibilityChange);
		};
	}, []);
	useEffect(() => {
		let timeout: number;
		if (!valueHidden) {
			timeout = window.setTimeout(() => {
				setValueHidden(true);
			}, AUTO_HIDE_INTERVAL);
		}
		return () => {
			if (timeout) {
				clearTimeout(timeout);
			}
		};
	}, [valueHidden]);
	return (
		<div className="flex flex-col flex-nowrap items-stretch gap-2 bg-white border border-solid border-gray-60 rounded-lg overflow-hidden py-4 px-5">
			<div className="break-all relative">
				{valueHidden ? null : (
					<div className="absolute top-0">
						<Text variant="pBody" weight="medium" color="steel-darker">
							{typeof value === 'string'
								? value
								: value.map((aValue, index) => (
										<span key={index}>{(index > 0 ? ' ' : '') + aValue}</span>
								  ))}
						</Text>
					</div>
				)}
				<div className={cx('flex flex-col gap-1.5', valueHidden ? 'visible' : 'invisible')}>
					<div className="h-3.5 bg-gray-40 rounded-md" />
					<div className="h-3.5 bg-gray-40 rounded-md" />
					<div className="h-3.5 bg-gray-40 rounded-md w-1/2" />
				</div>
			</div>
			<div className="flex flex-row flex-nowrap items-center justify-between">
				<div>
					{!hideCopy ? (
						<Link
							color="heroDark"
							weight="medium"
							size="body"
							text="Copy"
							before={<Copy16 className="text-base leading-none" />}
							onClick={copyCallback}
						/>
					) : null}
				</div>
				<div>
					<Link
						color="steelDark"
						size="base"
						weight="medium"
						text={valueHidden ? <EyeClose16 className="block" /> : <EyeOpen16 className="block" />}
						onClick={() => setValueHidden((v) => !v)}
					/>
				</div>
			</div>
		</div>
	);
}
