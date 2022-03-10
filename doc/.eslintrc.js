module.exports = {
  root: true,
  env: {
    browser: true,
    node: true,
  },
  extends: [
    '@nuxtjs/eslint-config-typescript',
    "@vue/prettier",
    'plugin:nuxt/recommended',
    'prettier',
  ],
  parserOptions: {
    parser: "babel-eslint"
  },
  plugins: [],
  // add your custom rules here
  rules: {},
}
