// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import createMDX from 'fumadocs-mdx/config';

const withMDX = createMDX();

/** @type {import('next').NextConfig} */
const config = {
	reactStrictMode: true,
};

export default withMDX(config);
