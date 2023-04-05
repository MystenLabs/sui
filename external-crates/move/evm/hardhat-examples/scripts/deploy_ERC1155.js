async function main() {
  const [deployer] = await ethers.getSigners();

  console.log("Deploying contracts with the account:", deployer.address);

  const weiAmount = (await deployer.getBalance()).toString();

  console.log("Account balance:", (await ethers.utils.formatEther(weiAmount)));

  const NFT = await ethers.getContractFactory("ERC1155Tradable"); // A Move contract
  const nft = await NFT.deploy(
    "Move-on-EVM NFT", // name
    "MFT", // symbol
    "https://bafybeigjlugkiakvejhgfupjdl66e77wlbjznksraj7w2g3djtjdeuhl74.ipfs.nftstorage.link/"); // baseURI
  console.log("Semi-NFT address:", nft.address);
}

main()
  .then(() => process.exit(0))
  .catch((error) => {
    console.error(error);
    process.exit(1);
});
