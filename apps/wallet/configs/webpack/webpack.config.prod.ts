// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import type { Configuration } from 'webpack';
import { merge } from 'webpack-merge';

import configCommon from './webpack.config.common';

const configProd: Configuration = {
	mode: 'production',
	devtool: 'source-map',
};

async function getConfig() {
	return merge(await configCommon(), configProd);
}

export default getConfig;
