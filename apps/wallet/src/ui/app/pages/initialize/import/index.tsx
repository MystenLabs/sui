// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useState } from 'react';

import StepOne from './steps/StepOne';
import StepTwo from './steps/StepTwo';
import { CardLayout } from '_app/shared/card-layout';

const initialValues = {
	mnemonic: Array.from({ length: 12 }, () => ''),
	password: '',
	confirmPassword: '',
};

const allSteps = [StepOne, StepTwo];

export type ImportValuesType = typeof initialValues;
export type ImportPageProps = {
	mode?: 'import' | 'forgot';
};
export function ImportPage({ mode = 'import' }: ImportPageProps) {
	const [data, _setData] = useState<ImportValuesType>(initialValues);
	const [step, _setStep] = useState(0);
	const totalSteps = allSteps.length;
	const StepForm = step < totalSteps ? allSteps[step] : null;
	return (
		<CardLayout
			headerCaption={mode === 'import' ? 'Wallet Setup' : undefined}
			title={mode === 'import' ? 'Import an Existing Wallet' : 'Reset Password for This Wallet'}
		>
			{StepForm ? (
				<div className="mt-7.5 flex flex-col flex-nowrap items-stretch flex-1 flex-grow w-full">
					<StepForm
						next={async (_data, _stepIncrement) => {
							throw new Error('Not implemented yet');
							// disable for now
							// const nextStep = step + stepIncrement;
							// if (nextStep >= totalSteps) {
							// 	await onHandleSubmit(data);
							// }
							// setData(data);
							// if (nextStep < 0) {
							// 	return;
							// }
							// setStep(nextStep);
						}}
						data={data}
						mode={mode}
					/>
				</div>
			) : null}
		</CardLayout>
	);
}

function _getSourceFlowForAmplitude(mode: 'import' | 'forgot') {
	switch (mode) {
		case 'import':
			return 'Import existing account';
		case 'forgot':
			return 'Reset password';
		default:
			throw new Error('Encountered unknown mode');
	}
}
