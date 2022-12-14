// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/** @type {import('tailwindcss').Config} */
module.exports = {
  content: [
    './pages/**/*.{js,ts,jsx,tsx}',
    './components/**/*.{js,ts,jsx,tsx}',
    './app/**/*.{js,ts,jsx,tsx}',
  ],
  theme: {
    screens: {
      'sm': '640px',
      'md': '768px',
      'lg': '1024px',
      'lg2': '1140px',
      'xl': '1280px',
      '2xl': '1536px',
    },
    colors: {
      'black': '#000000',
      'white': '#ffffff',
      'blue': '#1fb6ff',
      'sui-sky': '#6fbcf0',
      'sui-ocean': '#2a4362',
      'sui-ocean-dark': '#162a43',
      'ocean-darker': '#0d1520',
      'faint-blue': '#e6effe',
      'sui-text-dark': '#111111',
      'sui-text-light': '#76839d',
      'gray-dark': '#273444',
      'gray': '#8492a6',
      'gray-light': '#d3dce6',
      'amber': "#e32c22",
      'success': '#74b72e',
      'failure': "#b90e0a",
    },
    extend: {},
  },
  plugins: [
    require( 'tailwindcss' ),
		require( 'precss' ),
		require( 'autoprefixer' )
  ],
}