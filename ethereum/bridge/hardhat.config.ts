// import config before anything else
import { config as dotEnvConfig } from "dotenv";
import "@nomicfoundation/hardhat-ethers";
import "@nomicfoundation/hardhat-toolbox";
import "@openzeppelin/hardhat-upgrades";
import { HardhatUserConfig } from "hardhat/config";

dotEnvConfig();

// read MNEMONIC from file or from env variable
const mnemonic = process.env.MNEMONIC;
// read X-goog-api-key from file or from env variable
const xGoogApiKey = process.env.X_GOOG_API_KEY!;

const config: HardhatUserConfig = {
  solidity: {
    version: "0.8.20",
    settings: {
      optimizer: {
        enabled: true,
        runs: 200,
      },
      viaIR: true,
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
    sepoliasui: {
      url: `https://eth-rpc.testnet.sui.io:443`,
      accounts: { mnemonic: mnemonic },
    },
    ethsui: {
      accounts: { mnemonic: mnemonic },
      url: `http://json-rpc.ap1cmylexnxy1eadewp3v97e8.blockchainnodeengine.com`,
      // Use the X-goog-api-key as an http header
      httpHeaders: {
        "X-goog-api-key": xGoogApiKey,
      },
    },
    coverage: {
      url: "http://127.0.0.1:8555",
    },
  },
};

export default config;
