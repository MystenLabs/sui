// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { ArrowRight16 } from '@mysten/icons';
import { Formik, Form } from 'formik';
import * as Yup from 'yup';

import { Button } from '_app/shared/ButtonUI';
import FieldLabel from '_app/shared/field-label';
import Alert from '_components/alert';
import { mnemonicValidation } from '_pages/initialize/import/validation';
import { PasswordInputField } from '_src/ui/app/shared/input/password';
import { Text } from '_src/ui/app/shared/text';

import type { StepProps } from '.';

const validationSchema = Yup.object({
	mnemonic: mnemonicValidation,
});

function findNextWordInput(element: HTMLElement) {
	if (element.parentElement?.parentElement?.nextElementSibling) {
		const nextElement = element.parentElement?.parentElement?.nextElementSibling.children
			.item(1)
			?.children.item(0);
		if (nextElement instanceof HTMLInputElement) {
			return nextElement;
		}
	}
	return null;
}

export default function StepOne({ next, data, mode }: StepProps) {
	return (
		<Formik
			initialValues={data}
			validationSchema={validationSchema}
			validateOnMount
			onSubmit={async (values) => {
				await next(values, 1);
			}}
			enableReinitialize={true}
			validateOnChange
			validateOnBlur
		>
			{({ isSubmitting, touched, errors, values: { mnemonic }, isValid, setFieldValue }) => (
				<Form className="flex flex-col flex-nowrap items-stretch flex-1 flex-grow justify-between">
					<FieldLabel txt="Enter your 12-word Recovery Phrase">
						<div className="grid grid-cols-2 gap-x-2 gap-y-2.5 mt-1.5">
							{mnemonic.map((_, index) => {
								return (
									<div key={index} className="flex flex-col flex-nowrap gap-1.5 items-center">
										<Text variant="captionSmall" weight="medium" color="steel-darker">
											{index + 1}
										</Text>
										<PasswordInputField
											name={`mnemonic.${index}`}
											disabled={isSubmitting}
											onKeyDown={(e) => {
												if (e.key === ' ') {
													e.preventDefault();
													findNextWordInput(e.target as HTMLElement)?.focus();
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
													const newMnemonic = [...mnemonic];
													const wordsToPaste = words.slice(0, mnemonic.length - pasteIndex);
													newMnemonic.splice(
														pasteIndex,
														wordsToPaste.length,
														...words.slice(0, mnemonic.length - pasteIndex),
													);
													setFieldValue('mnemonic', newMnemonic);
												}
											}}
										/>
									</div>
								);
							})}
						</div>
					</FieldLabel>
					<div className="bg-sui-lightest flex flex-col flex-nowrap items-stretch gap-2.5 sticky -bottom-7.5 px-7.5 pb-7.5 pt-4.5 -mx-7.5 -mb-7.5">
						{touched.mnemonic && typeof errors.mnemonic === 'string' && (
							<Alert>{errors.mnemonic}</Alert>
						)}
						<Button
							type="submit"
							disabled={isSubmitting || !isValid}
							variant="primary"
							size="tall"
							loading={isSubmitting}
							text={mode === 'forgot' ? 'Next' : 'Continue'}
							after={<ArrowRight16 />}
						/>
					</div>
				</Form>
			)}
		</Formik>
	);
}
