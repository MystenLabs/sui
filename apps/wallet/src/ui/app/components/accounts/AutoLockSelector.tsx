// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useEffect } from 'react';
import { useFormContext } from 'react-hook-form';
import { z } from 'zod';

import { CheckboxField } from '../../shared/forms/CheckboxField';
import { Input } from '../../shared/forms/controls/Input';
import FormField from '../../shared/forms/FormField';
import { SelectField } from '../../shared/forms/SelectField';

const lockIntervals = [
	{ id: 'day', label: 'Day' },
	{ id: 'hour', label: 'Hour' },
	{ id: 'minute', label: 'Minute' },
];
const lockIntervalsPlural = [
	{ id: 'day', label: 'Days' },
	{ id: 'hour', label: 'Hours' },
	{ id: 'minute', label: 'Minutes' },
];

export const zodSchema = z.object({
	autoLock: z
		.object({
			enabled: z.boolean(),
			timer: z.coerce.number().int('Only integer numbers allowed'),
			interval: z.enum(['day', 'hour', 'minute']),
		})
		.refine(({ enabled, timer }) => !enabled || timer > 0, {
			message: 'Minimum of 1 minute is allowed',
			path: ['timer'],
		}),
});

type AutoLockSelectorProps = {
	disabled?: boolean;
};

export function AutoLockSelector({ disabled }: AutoLockSelectorProps) {
	const { register, watch, trigger } = useFormContext();
	const timer = watch('autoLock.timer');
	const timerEnabled = watch('autoLock.enabled');
	useEffect(() => {
		const { unsubscribe } = watch((_, { name, type }) => {
			if (name === 'autoLock.enabled' && type === 'change') {
				trigger('autoLock.timer');
			}
		});
		return unsubscribe;
	}, [watch, trigger]);
	return (
		<div className="flex flex-col gap-4">
			<CheckboxField
				name="autoLock.enabled"
				label="Auto-lock after I am inactive for"
				disabled={disabled}
			/>
			<FormField name="autoLock.timer">
				<div className="flex items-start justify-between gap-2">
					<Input
						disabled={disabled || !timerEnabled}
						type="number"
						{...register('autoLock.timer')}
						data-testid="auto-lock-timer"
					/>
					<SelectField
						disabled={disabled || !timerEnabled}
						name="autoLock.interval"
						options={Number(timer) === 1 ? lockIntervals : lockIntervalsPlural}
					/>
				</div>
			</FormField>
		</div>
	);
}
