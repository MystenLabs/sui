// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { useStore } from '@nanostores/react';
import type { ReactNode } from 'react';
import { createContext, useContext } from 'react';

import type { EnokiFlow } from './EnokiFlow.js';

const EnokiFlowContext = createContext<EnokiFlow | null>(null);

// TODO: Flatten props and construct an instance ourself.
export function EnokiFlowProvider({
	children,
	enokiFlow,
}: {
	children: ReactNode;
	enokiFlow: EnokiFlow;
}) {
	return <EnokiFlowContext.Provider value={enokiFlow}>{children}</EnokiFlowContext.Provider>;
}

// TODO: Should this just subscribe to the store too?
export function useEnokiFlow() {
	const context = useContext(EnokiFlowContext);
	if (!context) {
		throw new Error('Missing `EnokiFlowContext` provider');
	}
	return context;
}

export function useEnokiFlowState() {
	const flow = useEnokiFlow();
	const state = useStore(flow.$state);
	const initialized = useStore(flow.$initialized);
	return { ...state, initialized };
}
