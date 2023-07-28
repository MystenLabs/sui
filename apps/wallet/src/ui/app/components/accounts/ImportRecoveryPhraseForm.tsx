// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useZodForm } from '@mysten/core';
import { z } from 'Zod';
import { type SubmitHandler } from 'react-hook-form';
import { useNavigate } from 'react-router-dom';
import Alert from '../alert';
import { Button } from '_app/shared/ButtonUI';
import { normalizeMnemonics, validateMnemonics } from '_src/shared/utils/bip39';
import { PasswordInput } from '_src/ui/app/shared/forms/controls/PasswordInput';
import { Text } from '_src/ui/app/shared/text';

const RECOVERY_PHRASE_WORD_COUNT = 12;

const formSchema = z.object({
	recoveryPhrase: z
		.array(z.string().trim())
		.length(RECOVERY_PHRASE_WORD_COUNT)
		.transform((recoveryPhrase) => normalizeMnemonics(recoveryPhrase.join(' ')).split(' '))
		.refine((recoveryPhrase) => validateMnemonics(recoveryPhrase.join(' ')), {
			message: 'Recovery Passphrase is invalid',
		}),
});

type FormValues = z.infer<typeof formSchema>;

type ImportRecoveryPhraseFormProps = {
	submitButtonText: string;
	cancelButtonText: string;
	useFieldSet?: boolean;
	onSubmit: SubmitHandler<FormValues>;
};

export function ImportRecoveryPhraseForm({
	submitButtonText,
	cancelButtonText,
	useFieldSet = false,
	onSubmit,
}: ImportRecoveryPhraseFormProps) {
	const {
		register,
		formState: { errors, isSubmitting, isValid, touchedFields },
		handleSubmit,
		setValue,
		getValues,
		trigger,
	} = useZodForm({
		mode: 'all',
		reValidateMode: 'onChange',
		schema: formSchema,
		defaultValues: {
			recoveryPhrase: Array.from({ length: RECOVERY_PHRASE_WORD_COUNT }, () => ''),
		},
	});
	const navigate = useNavigate();
	const recoveryPhrase = getValues('recoveryPhrase');
	const formContent = (
		<div className="grid grid-cols-2 gap-x-2 gap-y-2.5 overflow-auto">
			{recoveryPhrase.map((_, index) => {
				const recoveryPhraseId = `recoveryPhrase.${index}` as const;
				return (
					<label key={index} className="flex flex-col gap-1.5 items-center">
						<Text variant="captionSmall" weight="medium" color="steel-darker">
							{index + 1}
						</Text>
						<PasswordInput
							disabled={isSubmitting}
							onKeyDown={(e) => {
								if (e.key === ' ') {
									e.preventDefault();
									const nextInput = document.getElementsByName(`recoveryPhrase.${index + 1}`)[0];
									nextInput?.focus();
								}
							}}
							onPaste={async (e) => {
								const inputText = e.clipboardData.getData('text');
								const words = inputText
									.trim()
									.split(/\W/)
									.map((aWord) => aWord.trim())
									.filter(String);

								if (words.length > 1) {
									e.preventDefault();
									const pasteIndex = words.length === recoveryPhrase.length ? 0 : index;
									const wordsToPaste = words.slice(0, recoveryPhrase.length - pasteIndex);
									const newRecoveryPhrase = [...recoveryPhrase];
									newRecoveryPhrase.splice(
										pasteIndex,
										wordsToPaste.length,
										...words.slice(0, recoveryPhrase.length - pasteIndex),
									);
									setValue('recoveryPhrase', newRecoveryPhrase);
									trigger('recoveryPhrase');
								}
							}}
							id={recoveryPhraseId}
							{...register(recoveryPhraseId)}
						/>
					</label>
				);
			})}
		</div>
	);

	return (
		<form
			className="flex flex-col justify-between relative h-full"
			onSubmit={handleSubmit(onSubmit)}
		>
			{useFieldSet ? (
				<fieldset className="border-0 m-0 p-0">
					<legend className="pl-2.5">
						<Text variant="pBody" color="steel-darker" weight="semibold">
							Enter your 12-word Recovery Phrase
						</Text>
					</legend>
					<div className="mt-3">{formContent}</div>
				</fieldset>
			) : (
				formContent
			)}
			<div className="flex flex-col gap-2.5 pt-3 bg-sui-lightest sticky -bottom-7.5 px-6 pb-7.5 -mx-6 -mb-7.5">
				{touchedFields.recoveryPhrase && errors.recoveryPhrase && (
					<Alert>{errors.recoveryPhrase.message}</Alert>
				)}
				<div className="flex gap-2.5">
					<Button
						variant="outline"
						size="tall"
						text={cancelButtonText}
						onClick={() => navigate(-1)}
					/>
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
		</form>
	);
}

type RecoveryPhraseInputGroupProps = {};

function RecoveryPhraseInputGroup() {
	return (
		<div className="grid grid-cols-2 gap-x-2 gap-y-2.5 overflow-auto">
			{mnemonic.map((_, index) => {
				const mnemonicId = `mnemonic.${index}` as const;
				return (
					<label key={index} className="flex flex-col gap-1.5 items-center">
						<Text variant="captionSmall" weight="medium" color="steel-darker">
							{index + 1}
						</Text>
						<PasswordInput
							disabled={isSubmitting}
							onKeyDown={(e) => {
								if (e.key === ' ') {
									e.preventDefault();
									const nextInput = document.getElementsByName(`mnemonic.${index + 1}`)[0];
									nextInput?.focus();
								}
							}}
							onPaste={async (e) => {
								const inputText = e.clipboardData.getData('text');
								const words = inputText
									.trim()
									.split(/\W/)
									.map((aWord) => aWord.trim())
									.filter(String);

								if (words.length > 1) {
									e.preventDefault();
									const pasteIndex = words.length === mnemonic.length ? 0 : index;
									const wordsToPaste = words.slice(0, mnemonic.length - pasteIndex);
									const newMnemonic = [...mnemonic];
									newMnemonic.splice(
										pasteIndex,
										wordsToPaste.length,
										...words.slice(0, mnemonic.length - pasteIndex),
									);
									setValue('mnemonic', newMnemonic);
									trigger('mnemonic');
								}
							}}
							id={mnemonicId}
							{...register(mnemonicId)}
						/>
					</label>
				);
			})}
		</div>
	);
}
