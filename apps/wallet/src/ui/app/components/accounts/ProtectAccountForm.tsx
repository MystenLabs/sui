// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Button } from '_app/shared/ButtonUI';
import { ToS_LINK } from '_src/shared/constants';
import { useZodForm } from '@mysten/core';
import { useEffect } from 'react';
import { type SubmitHandler } from 'react-hook-form';
import { useNavigate } from 'react-router-dom';
import { z } from 'zod';
import zxcvbn from 'zxcvbn';

import { parseAutoLock, useAutoLockMinutes } from '../../hooks/useAutoLockMinutes';
import { CheckboxField } from '../../shared/forms/CheckboxField';
import { Form } from '../../shared/forms/Form';
import { TextField } from '../../shared/forms/TextField';
import { Link } from '../../shared/Link';
import { AutoLockSelector, zodSchema } from './AutoLockSelector';

function addDot(str: string | undefined) {
	if (str && !str.endsWith('.')) {
		return `${str}.`;
	}
	return str;
}

const formSchema = z
	.object({
		password: z
			.object({
				input: z
					.string()
					.nonempty('Required')
					.superRefine((val, ctx) => {
						const {
							score,
							feedback: { warning, suggestions },
						} = zxcvbn(val);
						if (score <= 2) {
							ctx.addIssue({
								code: z.ZodIssueCode.custom,
								message: `${addDot(warning) || 'Password is not strong enough.'}${
									suggestions ? ` ${suggestions.join(' ')}` : ''
								}`,
							});
						}
					}),
				confirmation: z.string().nonempty('Required'),
			})
			.refine(({ input, confirmation }) => input && confirmation && input === confirmation, {
				path: ['confirmation'],
				message: "Passwords don't match",
			}),
		acceptedTos: z.literal<boolean>(true, {
			errorMap: () => ({ message: 'Please accept Terms of Service to continue' }),
		}),
	})
	.merge(zodSchema);

export type FormValues = z.infer<typeof formSchema>;

type ProtectAccountFormProps = {
	submitButtonText: string;
	cancelButtonText?: string;
	onSubmit: SubmitHandler<FormValues>;
	displayToS?: boolean;
};

export function ProtectAccountForm({
	submitButtonText,
	cancelButtonText,
	onSubmit,
	displayToS,
}: ProtectAccountFormProps) {
	const autoLock = useAutoLockMinutes();
	const form = useZodForm({
		mode: 'all',
		schema: formSchema,
		values: {
			password: { input: '', confirmation: '' },
			acceptedTos: !!displayToS,
			autoLock: parseAutoLock(autoLock.data || null),
		},
	});
	const {
		watch,
		register,
		formState: { isSubmitting, isValid },
		trigger,
		getValues,
	} = form;
	const navigate = useNavigate();
	useEffect(() => {
		const { unsubscribe } = watch((_, { name, type }) => {
			if (name === 'password.input' && type === 'change' && getValues('password.confirmation')) {
				trigger('password.confirmation');
			}
		});
		return unsubscribe;
	}, [watch, trigger, getValues]);
	return (
		<Form className="flex flex-col gap-6 h-full" form={form} onSubmit={onSubmit}>
			<TextField
				autoFocus
				type="password"
				label="Create Account Password"
				{...register('password.input')}
			/>
			<TextField
				type="password"
				label="Confirm Account Password"
				{...register('password.confirmation')}
			/>
			<AutoLockSelector />
			<div className="flex-1" />
			<div className="flex flex-col gap-5">
				{displayToS ? null : (
					<CheckboxField
						name="acceptedTos"
						label={
							<div className="text-bodySmall whitespace-nowrap">
								I read and agreed to the{' '}
								<span className="inline-block">
									<Link
										href={ToS_LINK}
										beforeColor="steelDarker"
										color="suiDark"
										text="Terms of Services"
									/>
								</span>
							</div>
						}
					/>
				)}
				<div className="flex gap-2.5">
					{cancelButtonText ? (
						<Button
							variant="outline"
							size="tall"
							text={cancelButtonText}
							onClick={() => navigate(-1)}
						/>
					) : null}
					<Button
						type="submit"
						disabled={isSubmitting || !isValid}
						variant="primary"
						size="tall"
						loading={isSubmitting}
						text={submitButtonText}
					/>
				</div>
			</div>
		</Form>
	);
}
