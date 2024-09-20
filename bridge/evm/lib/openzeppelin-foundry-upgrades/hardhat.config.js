// This Hardhat config is used for generating documentation only.

require('@nomicfoundation/hardhat-foundry');
require('solidity-docgen');

/**
 * @type import('hardhat/config').HardhatUserConfig
 */
module.exports = {
  solidity: {
    version: "0.8.24",
  },
  docgen: require('./docs/config'),
};
