const hre = require("hardhat");

async function main() {
  console.log("Deploying MyNFT contract...");

  const MyNFT = await hre.ethers.getContractFactory("MyNFT");
  const myNFT = await MyNFT.deploy();

  await myNFT.waitForDeployment();

  const address = await myNFT.getAddress();
  console.log(`✅ MyNFT deployed to: ${address}`);

  const [deployer] = await hre.ethers.getSigners();
  const mintPrice = await myNFT.mintPrice();
  const maxSupply = await myNFT.MAX_SUPPLY();

  console.log("\nNFT Collection Info:");
  console.log("===================");
  console.log(`Deployer: ${deployer.address}`);
  console.log(`Mint Price: ${hre.ethers.formatEther(mintPrice)} ETH`);
  console.log(`Max Supply: ${maxSupply.toString()} NFTs`);
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
