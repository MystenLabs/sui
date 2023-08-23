// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { get, set } from 'idb-keyval';
import { type ReactNode, createContext, useCallback, useEffect, useState, useContext } from 'react';

const NICKNAMES_MAPPING = 'nicknames-mapping';

type NicknamesMap = Record<string, string>;

interface NicknameContext {
	// {address: nickname}
	accountNicknames: NicknamesMap;
	setAccountNickname: (address: string, nickname: string) => void;
}

export const NicknamesContext = createContext<NicknameContext>({
	accountNicknames: {},
	setAccountNickname: () => {},
});

export const NicknamesProvider = ({ children }: { children: ReactNode }) => {
	const [accountNicknames, setAccountNicknames] = useState<NicknamesMap>({});
	useEffect(() => {
		(async () => {
			const nicknames = await get<NicknamesMap>(NICKNAMES_MAPPING);
			if (nicknames) {
				setAccountNicknames(nicknames);
			}
		})();
	}, []);

	const setAccountNickname = useCallback(
		async (address: string, nickname: string) => {
			const newNicknamesMapping = {
				...accountNicknames,
				[address]: nickname,
			};
			try {
				setAccountNicknames(newNicknamesMapping);
				await set(NICKNAMES_MAPPING, newNicknamesMapping);
			} catch (error) {
				// Restore the nicknames back to what they were before
				setAccountNicknames(accountNicknames);
				await set(NICKNAMES_MAPPING, accountNicknames);
			}
		},
		[accountNicknames],
	);

	return (
		<NicknamesContext.Provider
			value={{
				accountNicknames,
				setAccountNickname,
			}}
		>
			{children}
		</NicknamesContext.Provider>
	);
};

export const useAccountNicknames = () => {
	return useContext(NicknamesContext);
};
