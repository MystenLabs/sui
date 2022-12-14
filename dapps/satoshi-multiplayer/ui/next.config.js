// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

const path = require('path');
const webpack = require('webpack');
const MiniCssExtractPlugin = require('mini-css-extract-plugin');
const TailwindCSS = require('tailwindcss');

/**
 * @type {import('next').NextConfig}
 */
const nextConfig = {
  reactStrictMode: true,
  trailingSlash: true,
  webpack(config) {
    // Add a rule to handle .scss files
    config.module.rules.push({
      test: /\.scss$/,
      use: [
        MiniCssExtractPlugin.loader,
        'css-loader',
        {
          loader: 'postcss-loader',
          options: {
            postcssOptions: {
              plugins: [TailwindCSS("./tailwind.config.js")]
            },
            execute: true,
            sourceMap: true,
            implementation: require('postcss'),
          },
        },
        'sass-loader',
      ],
    });
    // Add the MiniCssExtractPlugin to the plugins array
    config.plugins.push(new MiniCssExtractPlugin());

    return config;
  },
  webpack(config) {
    config.module.rules.push({
      test: /\.svg$/i,
      issuer: /\.[jt]sx?$/,
      use: ['@svgr/webpack'],
    })

    return config;
  },
};

module.exports = nextConfig;