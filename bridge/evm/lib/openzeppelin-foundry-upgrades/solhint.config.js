const customRules = require('./scripts/solhint-custom');

console.log('Using custom rules:', JSON.stringify(customRules, null, 2));

const rules = [
  'no-unused-vars',
  'const-name-snakecase',
  'contract-name-camelcase',
  'event-name-camelcase',
  'func-name-mixedcase',
  'func-param-name-mixedcase',
  'modifier-name-mixedcase',
  'var-name-mixedcase',
  'imports-on-top',
  'no-global-import',
  ...customRules.map(r => `openzeppelin/${r.ruleId}`),
];

console.log('Using rules:', JSON.stringify(rules, null, 2));

module.exports = {
  plugins: ['openzeppelin'],
  rules: Object.fromEntries(rules.map(r => [r, 'error'])),
};
