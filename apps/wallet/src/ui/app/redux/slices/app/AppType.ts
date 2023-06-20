// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

export enum AppType {
	unknown,
	fullscreen,
	popup,
}

export function getFromLocationSearch(search: string) {
	if (/type=popup/.test(window.location.search)) {
		return AppType.popup;
	}
	return AppType.fullscreen;
}
