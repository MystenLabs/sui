// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { Command } from 'cmdk';
import {
	type ComponentProps,
	type ReactNode,
	type RefObject,
	createContext,
	useCallback,
	useContext,
	useEffect,
	useRef,
	useState,
} from 'react';

import { Text } from './Text';
import { LoadingIndicator } from './LoadingIndicator';
import { useOnClickOutside } from './hooks/useOnClickOutside';

export type ComboboxItem = {
	value: string;
	label: string;
	after?: ReactNode;
};

type ComboboxItemProps = {
	item: ComboboxItem;
	onSelect(): void;
};

function ComboboxItem({ item, onSelect }: ComboboxItemProps) {
	return (
		<Command.Item
			value={item.value}
			className="group mb-2 cursor-pointer rounded-md px-2 py-1.5 last:mb-0 data-[selected]:bg-sui/10 data-[selected]:shadow-sm"
			onSelect={() => onSelect()}
		>
			<div className="flex w-full items-center justify-between">
				<div className="text-body font-medium text-steel-dark group-data-[selected]:text-hero">
					{item.label}
				</div>
				{item.after}
			</div>
		</Command.Item>
	);
}

const ComboboxContext = createContext<{
	inputRef: RefObject<HTMLInputElement>;
	listRef: RefObject<HTMLDivElement>;
	value: string;
	onValueChange(value: string): void;
	visible: boolean;
	setVisible(visible: boolean): void;
} | null>(null);

function useComboboxContext() {
	const ctx = useContext(ComboboxContext);
	if (!ctx) {
		throw new Error('Missing Context');
	}
	return ctx;
}

export function ComboboxInput(props: ComponentProps<typeof Command.Input>) {
	const { inputRef, value, onValueChange, setVisible } = useComboboxContext();

	return (
		<Command.Input
			ref={inputRef}
			value={value}
			onValueChange={onValueChange}
			spellCheck={false}
			autoComplete="off"
			onFocus={() => setVisible(true)}
			{...props}
		/>
	);
}

type ComboboxListProps<T extends ComboboxItem> = {
	isLoading?: boolean;
	showResultsCount?: boolean;
	options: T[];
	onSelect(value: T): void;
};

export function ComboboxList<T extends ComboboxItem = ComboboxItem>({
	isLoading,
	showResultsCount,
	options,
	onSelect,
}: ComboboxListProps<T>) {
	const { visible, value, setVisible, onValueChange, listRef, inputRef } = useComboboxContext();

	if (!visible || !value) {
		return null;
	}

	return (
		<Command.List
			ref={listRef}
			className="absolute mt-1 w-full list-none rounded-md bg-white p-3.5 shadow-moduleOption h-fit max-h-verticalListLong z-10 overflow-scroll"
		>
			{showResultsCount && !isLoading && options.length > 0 && (
				<Command.Item className="text-left ml-1.5 pb-2" disabled>
					<Text variant="caption/semibold" color="gray-75" uppercase>
						{options.length}
						{options.length === 1 ? ' Result' : ' Results'}
					</Text>
				</Command.Item>
			)}

			{isLoading ? (
				<Command.Loading>
					<div className="flex items-center justify-center">
						<LoadingIndicator />
					</div>
				</Command.Loading>
			) : options.length > 0 ? (
				options.map((item) => (
					<ComboboxItem
						key={item.value}
						item={item}
						onSelect={() => {
							onSelect(item);
							onValueChange('');
							setVisible(false);
							inputRef.current?.blur();
						}}
					/>
				))
			) : (
				<Command.Item className="flex items-center justify-center" disabled>
					<Text variant="body/medium" color="steel-darker" italic>
						No Results
					</Text>
				</Command.Item>
			)}
		</Command.List>
	);
}

type Props = {
	value: string;
	onValueChange(value: string): void;
	children: ReactNode;
};

export function Combobox({ value, onValueChange, children }: Props) {
	const [visible, setVisible] = useState(false);
	const listRef = useRef<HTMLDivElement>(null);
	const inputRef = useRef<HTMLInputElement>(null);

	useEffect(() => {
		const handler = (e: KeyboardEvent) => {
			if (e.key === 'Escape') {
				setVisible(false);
				inputRef.current?.blur();
			}
		};

		document.addEventListener('keydown', handler);

		return () => {
			document.removeEventListener('keydown', handler);
		};
	}, []);

	const handleClickOutside = useCallback((e: MouseEvent | TouchEvent) => {
		if (listRef.current?.contains(e.target as Node)) {
			return;
		}
		setVisible(false);
	}, []);

	useOnClickOutside(inputRef, handleClickOutside);

	return (
		<ComboboxContext.Provider
			value={{
				listRef,
				inputRef,
				value,
				onValueChange,
				visible,
				setVisible,
			}}
		>
			<Command className="relative w-full" shouldFilter={false}>
				{children}
			</Command>
		</ComboboxContext.Provider>
	);
}
