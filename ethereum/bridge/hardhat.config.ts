// import config before anything else
import { config as dotEnvConfig } from "dotenv";
import "@nomicfoundation/hardhat-ethers";
import "@nomicfoundation/hardhat-toolbox";
import "@openzeppelin/hardhat-upgrades";
import { HardhatUserConfig } from "hardhat/config";

dotEnvConfig();

// read MNEMONIC from file or from env variable
let mnemonic = process.env.MNEMONIC!;
// read ALCHEMY_API_KEY from file or from env variable
let alchemyApiKey = process.env.ALCHEMY_API_KEY!;

const config: HardhatUserConfig = {
  solidity: {
    version: "0.8.20",
    settings: {
      optimizer: {
        enabled: true,
        runs: 200,
      },
    },
  },
  paths: {
    sources: "./contracts",
    tests: "./test",
    cache: "./cache",
    artifacts: "./artifacts",
  },
  mocha: {
    timeout: 40000,
  },
  networks: {
    hardhat: {},
    sepolia: {
      url: "https://sepolia.infura.io/v3/<key>",
      accounts: { mnemonic: mnemonic },
    },
    goerli: {
      url: `https://eth-goerli.alchemyapi.io/v2/${alchemyApiKey}`,
      accounts: { mnemonic: mnemonic },
    },
    sepoliasui: {
      url: `https://eth-rpc.testnet.sui.io:443`,
      accounts: { mnemonic: process.env.MNEMONIC! },
    },
    goerlihh: {
      url: "https://rpc.ankr.com/eth_goerli",
      chainId: 5,
    },
    coverage: {
      url: "http://127.0.0.1:8555",
    },
  },
  // defender: {
  //   apiKey: process.env.API_KEY,
  //   apiSecret: process.env.API_SECRET,
  // },
};

export default config;
