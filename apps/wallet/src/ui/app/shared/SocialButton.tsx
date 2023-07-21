// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { SocialFacebook24, SocialGoogle24, SocialMicrosoft24, SocialTwitch24 } from '@mysten/icons';
import { cva, type VariantProps } from 'class-variance-authority';
import { forwardRef, type FunctionComponent, type SVGProps, type Ref } from 'react';
import { ButtonOrLink, type ButtonOrLinkProps } from '../utils/ButtonOrLink';
import { Text } from '_app/shared/text';

const styles = cva(
	'w-full cursor-pointer rounded-xl inline-flex items-center justify-center gap-3 px-4 py-2 disabled:opacity-40 focus:opacity-80',
	{
		variants: {
			provider: {
				microsoft:
					'bg-white text-steel-dark border border-solid border-steel hover:border-steel-dark',
				google: 'bg-white text-steel-dark border border-solid border-steel hover:border-steel-dark',
				facebook: 'bg-[#1877F2] border-none text-white',
				twitch: 'bg-[#6441A5] border-none text-white',
			},
		},
	},
);

type StyleProps = VariantProps<typeof styles>;
type SocialSignInProvider = NonNullable<StyleProps['provider']>;

type SocialButtonProps = {
	showLabel?: boolean;
	provider: SocialSignInProvider;
} & Omit<ButtonOrLinkProps, 'className'> &
	StyleProps;

const socialSignInProviderInfo: Record<
	SocialSignInProvider,
	{ icon: FunctionComponent<SVGProps<SVGSVGElement>>; label: string }
> = {
	microsoft: {
		icon: SocialMicrosoft24,
		label: 'Sign in with Microsoft',
	},
	google: {
		icon: SocialGoogle24,
		label: 'Sign in with Google',
	},
	facebook: {
		icon: SocialFacebook24,
		label: 'Sign in with Facebook',
	},
	twitch: {
		icon: SocialTwitch24,
		label: 'Sign in with Twitch',
	},
};

export const SocialButton = forwardRef(
	(
		{ provider, showLabel = false, ...otherProps }: SocialButtonProps,
		forwardedRef: Ref<HTMLAnchorElement | HTMLButtonElement>,
	) => {
		const { icon: IconComponent, label } = socialSignInProviderInfo[provider];
		return (
			<ButtonOrLink
				ref={forwardedRef}
				className={styles({ provider })}
				aria-label={showLabel ? undefined : ''}
				{...otherProps}
			>
				<IconComponent width={24} height={24} />
				{showLabel && (
					<Text variant="pBodySmall" weight="semibold">
						{label}
					</Text>
				)}
			</ButtonOrLink>
		);
	},
);
