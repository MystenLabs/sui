// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import './ComputedField.css';

import { CheckIcon, CopyIcon } from '@radix-ui/react-icons';
import { IconButton, TextField, Tooltip } from '@radix-ui/themes';
import { ReactElement, useState } from 'react';
import toast from 'react-hot-toast';

/**
 * A TextField that displays a value that is already generated (cannot
 * be edited).
 *
 * If `value` is not provided, the `label` will be shown as a
 * placeholder, otherwise, the value will be shown, a tooltip will be
 * added with the label and value, and a copy button is included to
 * copy the value.
 */
export function ComputedField({ label, value }: { label: string; value?: string }): ReactElement {
	const tooltip = value ? `${label}: ${value}` : undefined;
	const [copied, setCopied] = useState(false);

	async function onClick() {
		await navigator.clipboard.writeText(value!!);
		setCopied(true);
		setTimeout(() => setCopied(false), 1000);
		toast.success('Copied ID to clipboard!');
	}

	const field = (
		<TextField.Root
			className="filledField"
			size="2"
			mb="2"
			value={value}
			placeholder={label}
			disabled={true}
		>
			{value && (
				<TextField.Slot side="right">
					<IconButton variant="ghost" onClick={onClick}>
						{copied ? <CheckIcon /> : <CopyIcon />}
					</IconButton>
				</TextField.Slot>
			)}
		</TextField.Root>
	);

	if (!tooltip) {
		return field;
	}

	return <Tooltip content={tooltip}>{field}</Tooltip>;
}
