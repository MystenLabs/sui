// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import cl from 'classnames';

import { type DisplayType } from './SuiApp';

import st from './SuiApp.module.scss';

export type SuiAppEmptyProps = {
	displayType: DisplayType;
};

export function SuiAppEmpty({ displayType }: SuiAppEmptyProps) {
	return (
		<div className={cl(st.suiApp, st.suiAppEmpty, st[displayType])}>
			<div className={st.icon}></div>
			<div className={st.info}>
				<div className={st.boxOne}></div>
				{displayType === 'full' && (
					<>
						<div className={st.boxTwo}></div>
						<div className={st.boxThree}></div>
					</>
				)}
			</div>
		</div>
	);
}
