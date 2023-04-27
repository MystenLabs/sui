// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

import { randomBytes } from '@noble/hashes/utils';
import SentryWebpackPlugin from '@sentry/webpack-plugin';
import { exec } from 'child_process';
import CopyPlugin from 'copy-webpack-plugin';
import DotEnv from 'dotenv-webpack';
import gitRevSync from 'git-rev-sync';
import HtmlWebpackPlugin from 'html-webpack-plugin';
import MiniCssExtractPlugin from 'mini-css-extract-plugin';
import { resolve } from 'path';
import { DefinePlugin, ProvidePlugin } from 'webpack';

import packageJson from '../../package.json';

import type { Configuration } from 'webpack';

function generateDateVersion(patch: number) {
    const sha = gitRevSync.short();
    const date = new Date();
    const version = [
        String(date.getUTCFullYear()).slice(2),
        String(date.getUTCMonth() + 1),
        String(date.getUTCDate()),
        patch,
    ].join('.');

    return {
        version,
        version_name: `${version} (${sha})`,
    };
}

const WALLET_BETA = process.env.WALLET_BETA === 'true';
const PATCH_VERISON = Number(process.env.PATCH_VERSION) || 0;

const PROJECT_ROOT = resolve(__dirname, '..', '..');
const CONFIGS_ROOT = resolve(PROJECT_ROOT, 'configs');
const SRC_ROOT = resolve(PROJECT_ROOT, 'src');
const OUTPUT_ROOT = resolve(PROJECT_ROOT, 'dist');
const TS_CONFIGS_ROOT = resolve(CONFIGS_ROOT, 'ts');
const IS_DEV = process.env.NODE_ENV === 'development';
const IS_PROD = process.env.NODE_ENV === 'production';
const TS_CONFIG_FILE = resolve(
    TS_CONFIGS_ROOT,
    `tsconfig.${IS_DEV ? 'dev' : 'prod'}.json`
);
const APP_NAME = WALLET_BETA
    ? 'Sui Wallet (BETA)'
    : IS_DEV
    ? 'Sui Wallet (DEV)'
    : 'Sui Wallet';

function loadTsConfig(tsConfigFilePath: string) {
    return new Promise<string>((res, rej) => {
        exec(
            `${resolve(
                PROJECT_ROOT,
                'node_modules',
                '.bin',
                'tsc'
            )} -p ${tsConfigFilePath} --showConfig`,
            (error, stdout, stderr) => {
                if (error || stderr) {
                    rej(error || stderr);
                }
                res(stdout);
            }
        );
    }).then(
        (tsContent) => JSON.parse(tsContent),
        (e) => {
            // eslint-disable-next-line no-console
            console.error(e);
            throw e;
        }
    );
}

async function generateAliasFromTs() {
    const tsConfigJSON = await loadTsConfig(TS_CONFIG_FILE);
    const {
        compilerOptions: { paths, baseUrl = './' },
    } = tsConfigJSON;
    const alias: Record<string, string> = {};
    if (paths) {
        Object.keys(paths).forEach((anAlias) => {
            const aliasPath = paths[anAlias][0];
            const adjAlias = anAlias.replace(/\/\*$/gi, '');
            const adjPath = (
                aliasPath.startsWith('./') || aliasPath.startsWith('../')
                    ? resolve(TS_CONFIGS_ROOT, baseUrl, aliasPath)
                    : aliasPath
            ).replace(/\/\*$/, '');
            alias[adjAlias] = adjPath;
        });
    }
    return alias;
}

const commonConfig: () => Promise<Configuration> = async () => {
    const alias = await generateAliasFromTs();
    const walletVersionDetails = generateDateVersion(PATCH_VERISON);
    const sentryAuthToken = process.env.SENTRY_AUTH_TOKEN;
    return {
        context: SRC_ROOT,
        entry: {
            background: './background',
            ui: './ui',
            'content-script': './content-script',
            'dapp-interface': './dapp-interface',
        },
        output: {
            path: OUTPUT_ROOT,
            clean: true,
        },
        stats: {
            preset: 'summary',
            timings: true,
            errors: true,
            warnings: true,
        },
        resolve: {
            extensions: ['.ts', '.tsx', '.js'],
            alias,
            fallback: {
                crypto: false,
                stream: require.resolve('stream-browserify'),
                buffer: require.resolve('buffer/'),
            },
        },
        module: {
            rules: [
                {
                    test: /\.(t|j)sx?$/,
                    loader: 'ts-loader',
                    options: {
                        configFile: TS_CONFIG_FILE,
                    },
                    exclude: /node_modules/,
                },
                {
                    test: /\.(s)?css$/i,
                    use: [
                        MiniCssExtractPlugin.loader,
                        {
                            loader: 'css-loader',
                            options: {
                                modules: {
                                    auto: true,
                                    localIdentName: IS_DEV
                                        ? '[name]__[local]__[hash:base64:8]'
                                        : '[hash:base64]',
                                    exportLocalsConvention: 'dashes',
                                },
                            },
                        },
                        'postcss-loader',
                        'sass-loader',
                    ],
                },
                {
                    test: /\.(png|jpg|jpeg|gif)$/,
                    type: 'asset/resource',
                },
                {
                    test: /\.svg$/i,
                    issuer: /\.[jt]sx?$/,
                    resourceQuery: { not: [/url/] },
                    use: ['@svgr/webpack'],
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
                            const manifestJson = {
                                ...JSON.parse(content.toString()),
                                ...walletVersionDetails,
                                name: APP_NAME,
                                description: packageJson.description,
                            };
                            return JSON.stringify(manifestJson, null, 4);
                        },
                    },
                ],
            }),
            new DotEnv({
                path: resolve(CONFIGS_ROOT, 'environment', '.env'),
                defaults: resolve(CONFIGS_ROOT, 'environment', '.env.defaults'),
                expand: true,
            }),
            new DefinePlugin({
                // This brakes bg service, js-sha3 checks if window is defined,
                // but it's not defined in background service.
                // TODO: check if this is worth investigating a fix and maybe do a separate build for UI and bg?
                // 'typeof window': JSON.stringify(typeof {}),
                'process.env.NODE_DEBUG': false,
                'process.env.WALLET_KEYRING_PASSWORD': JSON.stringify(
                    IS_DEV
                        ? 'DEV_PASS'
                        : Buffer.from(randomBytes(64)).toString('hex')
                ),
                'process.env.WALLET_BETA': WALLET_BETA,
                'process.env.APP_NAME': JSON.stringify(APP_NAME),
            }),
            new ProvidePlugin({
                Buffer: ['buffer', 'Buffer'],
            }),
            new SentryWebpackPlugin({
                org: 'mysten-labs',
                project: 'wallet',
                include: OUTPUT_ROOT,
                dryRun: !IS_PROD || !sentryAuthToken,
                authToken: sentryAuthToken,
                release: walletVersionDetails.version,
                silent: true,
            }),
        ],
    };
};

export default commonConfig;
