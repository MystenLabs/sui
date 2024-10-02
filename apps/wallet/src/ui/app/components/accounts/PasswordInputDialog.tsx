// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Button } from '_src/ui/app/shared/ButtonUI';
import {
	Dialog,
	DialogContent,
	DialogDescription,
	DialogFooter,
	DialogHeader,
	DialogTitle,
} from '_src/ui/app/shared/Dialog';
import { useZodForm } from '@mysten/core';
import { useState } from 'react';
import { toast } from 'react-hot-toast';
import { v4 as uuidV4 } from 'uuid';
import { z } from 'zod';

import { useAccountSources } from '../../hooks/useAccountSources';
import { useBackgroundClient } from '../../hooks/useBackgroundClient';
import { PasswordInput } from '../../shared/forms/controls/PasswordInput';
import { Form } from '../../shared/forms/Form';
import FormField from '../../shared/forms/FormField';
import { Link } from '../../shared/Link';

const formSchema = z.object({
	password: z.string().nonempty('Required'),
});

export type PasswordModalDialogProps = {
	onClose: () => void;
	open: boolean;
	showForgotPassword?: boolean;
	title: string;
	description: string;
	confirmText: string;
	cancelText: string;
	onSubmit: (password: string) => Promise<void> | void;
	verify?: boolean;
};

export function PasswordModalDialog({
	onClose,
	onSubmit,
	open,
	verify,
	showForgotPassword,
	title,
	description,
	confirmText,
	cancelText,
}: PasswordModalDialogProps) {
	const form = useZodForm({
		mode: 'all',
		schema: formSchema,
		defaultValues: {
			password: '',
		},
	});
	const {
		register,
		setError,
		reset,
		formState: { isSubmitting, isValid },
	} = form;
	const backgroundService = useBackgroundClient();
	const [formID] = useState(() => uuidV4());
	const { data: allAccountsSources } = useAccountSources();
	const hasMnemonicAccountsSources =
		allAccountsSources?.some(({ type }) => type === 'mnemonic') || false;
	return (
		<Dialog open={open}>
			<DialogContent onPointerDownOutside={(e) => e.preventDefault()}>
				<DialogHeader>
					<DialogTitle>{title}</DialogTitle>
					<DialogDescription asChild>
						<span className="sr-only">{description}</span>
					</DialogDescription>
				</DialogHeader>
				<Form
					form={form}
					id={formID}
					onSubmit={async ({ password }) => {
						try {
							if (verify) {
								await backgroundService.verifyPassword({ password });
							}
							try {
								await onSubmit(password);
								reset();
							} catch (e) {
								toast.error((e as Error).message || 'Something went wrong');
							}
						} catch (e) {
							setError(
								'password',
								{ message: (e as Error).message || 'Wrong password' },
								{ shouldFocus: true },
							);
						}
					}}
				>
					<label className="sr-only" htmlFor="password">
						Password
					</label>
					<FormField name="password">
						<PasswordInput {...register('password')} />
					</FormField>
				</Form>
				<DialogFooter>
					<div className="flex flex-col gap-3">
						<div className="flex gap-2.5">
							<Button variant="outline" size="tall" text={cancelText} onClick={onClose} />
							<Button
								type="submit"
								form={formID}
								disabled={isSubmitting || !isValid}
								variant="primary"
								size="tall"
								loading={isSubmitting}
								text={confirmText}
							/>
						</div>
						{showForgotPassword && hasMnemonicAccountsSources ? (
							<Link
								color="steelDark"
								weight="medium"
								size="bodySmall"
								text="Forgot Password?"
								to="/accounts/forgot-password"
								onClick={onClose}
							/>
						) : null}
					</div>
				</DialogFooter>
			</DialogContent>
		</Dialog>
	);
}
