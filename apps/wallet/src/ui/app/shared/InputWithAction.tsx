// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Text } from '_app/shared/text';
import LoadingIndicator from '_components/loading/LoadingIndicator';
import NumberInput from '_components/number-input';
import { cva, type VariantProps } from 'class-variance-authority';
import clsx from 'clsx';
import { useField, useFormikContext } from 'formik';
import type { ComponentProps, ReactNode } from 'react';
import { forwardRef } from 'react';

import Alert from '../components/alert';
import { Pill, type PillProps } from './Pill';

const styles = cva(
	[
		'transition flex flex-row items-center p-3 bg-white text-body font-semibold',
		'placeholder:text-gray-60 w-full pr-[calc(20%_+_24px)] shadow-button',
		'border-solid border border-gray-45 text-steel-darker hover:border-steel focus:border-steel',
		'disabled:border-gray-40 disabled:text-gray-55',
	],
	{
		variants: {
			rounded: {
				lg: 'rounded-2lg',
				md: 'rounded-md',
			},
			// TODO: handle dark outline Pill
			dark: {
				true: '',
				false: '',
			},
		},
		defaultVariants: {
			rounded: 'lg',
			dark: false,
		},
	},
);

type ActionButtonProps = {
	actionText?: string;
	onActionClicked?: PillProps['onClick'];
	actionType?: PillProps['type'];
	name: string;
	actionDisabled?: boolean | 'auto';
};

export type InputWithActionProps = VariantProps<typeof styles> &
	(
		| (Omit<ComponentProps<'input'>, 'className' | 'type'> & {
				type?: 'text' | 'number' | 'password' | 'email';
		  })
		| (Omit<ComponentProps<typeof NumberInput>, 'form' | 'field' | 'meta'> & {
				type: 'numberInput';
		  })
	) &
	ActionButtonProps;

export function InputWithAction({
	actionText,
	onActionClicked,
	actionType = 'submit',
	type,
	disabled = false,
	actionDisabled = false,
	name,
	dark,
	rounded,
	...props
}: InputWithActionProps) {
	const [field, meta] = useField(name);
	const form = useFormikContext();
	const { isSubmitting } = form;
	const isInputDisabled = isSubmitting || disabled;
	const isActionDisabled =
		actionDisabled === 'auto'
			? isInputDisabled || meta?.initialValue === meta?.value || !!meta?.error
			: actionDisabled;

	return (
		<>
			<div className="flex flex-row flex-nowrap items-center relative">
				{type === 'numberInput' ? (
					<NumberInput
						className={styles({ rounded })}
						allowNegative
						{...props}
						form={form}
						field={field}
						meta={meta}
						disabled={isInputDisabled}
					/>
				) : (
					<input
						type={type}
						disabled={isInputDisabled}
						{...field}
						{...props}
						className={styles({ rounded })}
					/>
				)}
				<div className="flex items-center justify-end absolute right-0 max-w-[20%] mx-3 overflow-hidden">
					<Pill
						text={actionText}
						type={actionType}
						disabled={isActionDisabled}
						loading={isSubmitting}
						onClick={onActionClicked}
						dark={dark}
					/>
				</div>
			</div>

			{(meta?.touched && meta?.error) || (meta.value !== '' && meta.error) ? (
				<div className="mt-3">
					<Alert>{meta?.error}</Alert>
				</div>
			) : null}
		</>
	);
}

const inputWithActionZodFormStyles = cva(
	[
		'transition flex flex-row items-center px-3 py-2 text-body font-semibold overflow-hidden',
		'placeholder:text-gray-60 w-full pr-[calc(20%_+_24px)]',
		'border-solid border text-steel-darker',
		'relative',
	],
	{
		variants: {
			rounded: {
				lg: 'rounded-2lg',
				md: 'rounded-md',
			},
			noBorder: {
				true: 'border-transparent',
				false: '',
			},
			disabled: {
				true: 'bg-gray-40',
				false: 'bg-white hover:border-steel focus:border-steel',
			},
		},
		defaultVariants: {
			rounded: 'lg',
			noBorder: false,
		},
		compoundVariants: [
			{
				noBorder: false,
				disabled: true,
				class: 'border-hero-darkest/10',
			},
			{
				noBorder: false,
				disabled: false,
				class: 'border-steel',
			},
		],
	},
);

type InputWithActionZodFormProps = VariantProps<typeof inputWithActionZodFormStyles> &
	(Omit<ComponentProps<'input'>, 'className' | 'type'> & {
		type?: 'text' | 'number' | 'password' | 'email';
	}) &
	ActionButtonProps & {
		errorString?: string;
		suffix?: ReactNode;
		prefix?: ReactNode;
		loading?: boolean;
		loadingText?: string;
		onActionClicked?: PillProps['onClick'];
		info?: ReactNode;
		actionDisabled?: boolean;
	};

export const InputWithActionButton = forwardRef<HTMLInputElement, InputWithActionZodFormProps>(
	(
		{
			actionText,
			onActionClicked,
			actionType = 'submit',
			type,
			disabled = false,
			actionDisabled = false,
			rounded,
			errorString,
			value,
			suffix,
			prefix,
			info,
			noBorder,
			loading,
			loadingText,
			...props
		},
		forwardRef,
	) => {
		const prefixContent = prefix ? (
			<span>
				<Text variant="body" color="steel">
					{prefix}
				</Text>
			</span>
		) : null;

		return (
			<>
				<div className={inputWithActionZodFormStyles({ rounded, noBorder, disabled })}>
					{prefixContent}
					<input
						{...props}
						value={value}
						autoFocus
						type={type}
						className={clsx(
							'bg-transparent z-10 border-none p-0 text-heading5 text-steel-darker font-semibold h-6 caret-hero',
							loading && 'text-transparent',
						)}
						disabled={disabled}
						ref={forwardRef}
					/>
					{loading && (
						<div className="absolute">
							<div className="flex items-center gap-1 text-steel">
								<LoadingIndicator color="inherit" />
								{loadingText && (
									<Text variant="body" color="steel">
										{loadingText}
									</Text>
								)}
							</div>
						</div>
					)}
					{suffix && value && (
						<div
							className={clsx(
								'absolute z-0 flex h-full max-w-full items-center border border-transparent',
								loading && 'text-transparent',
							)}
						>
							{prefixContent}
							<span className="invisible max-w-full text-heading5">{value}</span>
							<span className="ml-2 font-medium text-body text-steel">{suffix}</span>
						</div>
					)}

					{(onActionClicked || info) && (
						<div className="z-10 flex gap-2 items-center justify-end absolute mx-2 right-0 overflow-hidden bg-white">
							{info}
							{onActionClicked && (
								<Pill
									dark
									text={actionText}
									type={actionType}
									disabled={disabled || actionDisabled}
									onClick={onActionClicked}
								/>
							)}
						</div>
					)}
				</div>

				{errorString ? (
					<div className="mt-3">
						<Alert>{errorString}</Alert>
					</div>
				) : null}
			</>
		);
	},
);
