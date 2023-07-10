// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
import { type ReactNode, createContext, useContext, useEffect } from 'react';
import { createPortal } from 'react-dom';

export type GradientContainerContextType = {
	setVisible: (visible: boolean) => void;
};

export const GradientContainerContext = createContext<GradientContainerContextType | undefined>(
	undefined,
);

export type GradientContainerProps = {
	children: ReactNode;
};

export function GradientContainer({ children }: GradientContainerProps) {
	const gradientContext = useContext(GradientContainerContext);
	const setVisible = gradientContext?.setVisible;
	useEffect(() => {
		setVisible?.(true);
		return () => {
			setVisible?.(false);
		};
	}, [setVisible]);
	const gradientContainer = document.getElementById('gradient-content-container');
	if (!gradientContainer) {
		return null;
	}
	return createPortal(children, gradientContainer);
}
