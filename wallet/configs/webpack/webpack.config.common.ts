// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import CopyPlugin from 'copy-webpack-plugin';
import HtmlWebpackPlugin from 'html-webpack-plugin';
import MiniCssExtractPlugin from 'mini-css-extract-plugin';
import { resolve } from 'path';

import packageJson from '../../package.json';

import type { Configuration } from 'webpack';

const APP_NAME = 'Sui Wallet';
const PROJECT_ROOT = resolve(__dirname, '..', '..');
const CONFIGS_ROOT = resolve(PROJECT_ROOT, 'configs');
const SRC_ROOT = resolve(PROJECT_ROOT, 'src');
const OUTPUT_ROOT = resolve(PROJECT_ROOT, 'dist');

const tsConfigFilename = `tsconfig.${
    process.env.NODE_ENV === 'development' ? 'dev' : 'prod'
}.json`;

const commonConfig: Configuration = {
    context: SRC_ROOT,
    entry: {
        background: './background',
        ui: './ui',
    },
    output: {
        path: OUTPUT_ROOT,
        clean: true,
    },
    stats: {
        preset: 'summary',
        timings: true,
        errors: true,
    },
    resolve: {
        extensions: ['.ts', '.tsx'],
    },
    module: {
        rules: [
            {
                test: /\.(t|j)sx?$/,
                loader: 'ts-loader',
                options: {
                    configFile: resolve(CONFIGS_ROOT, 'ts', tsConfigFilename),
                },
            },
            {
                test: /\.(s)?css$/i,
                use: [
                    MiniCssExtractPlugin.loader,
                    'css-loader',
                    'postcss-loader',
                    'sass-loader',
                ],
            },
            {
                test: /\.(png|jpg|jpeg|gif)$/,
                type: 'asset/resource',
            },
        ],
    },
    plugins: [
        new MiniCssExtractPlugin(),
        new HtmlWebpackPlugin({
            chunks: ['ui'],
            filename: 'ui.html',
            template: resolve(SRC_ROOT, 'ui', 'index.template.html'),
            title: APP_NAME,
        }),
        new CopyPlugin({
            patterns: [
                {
                    from: resolve(SRC_ROOT, 'manifest', 'icons', '**', '*'),
                },
                {
                    from: resolve(SRC_ROOT, 'manifest', 'manifest.json'),
                    to: resolve(OUTPUT_ROOT, '[name][ext]'),
                    transform: (content) => {
                        const { description, version } = packageJson;
                        const manifestJson = {
                            ...JSON.parse(content.toString()),
                            name: APP_NAME,
                            description,
                            version,
                        };
                        return JSON.stringify(manifestJson, null, 4);
                    },
                },
            ],
        }),
    ],
};

export default commonConfig;
