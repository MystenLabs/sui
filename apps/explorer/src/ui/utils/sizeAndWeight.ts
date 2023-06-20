// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

interface SizeAndWeightProps {
	size?: string | null;
	weight?: string | null;
}
export type SizeAndWeightVariant<T extends SizeAndWeightProps> = `${NonNullable<
	T['size']
>}/${NonNullable<T['weight']>}`;

export function parseVariant<T extends SizeAndWeightProps>(variant: string) {
	return variant.split('/') as [T['size'], T['weight']];
}
