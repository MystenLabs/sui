import { ethers, upgrades } from "hardhat";

async function main() {
  const validators = [
    {
      addr: "0x5567f54B29B973343d632f7BFCe9507343D41FCa",
      weight: 1000,
    },
    {
      addr: "0x6E78914596C4c3fA605AD25A932564c753353DcC",
      weight: 1000,
    },
  ];

  const Bridge = await ethers.getContractFactory("Bridge");
  console.log("Deploying Bridge...");
  const bridge = await upgrades.deployProxy(Bridge, [validators], {
    initializer: "initialize",
  });
  await bridge.waitForDeployment();
  console.log("Bridge deployed to:", await bridge.getAddress());
}

// We recommend this pattern to be able to use async/await everywhere
// and properly handle errors.
main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
