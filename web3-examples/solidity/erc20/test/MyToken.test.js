const { expect } = require("chai");
const { ethers } = require("hardhat");

describe("MyToken", function () {
  let myToken;
  let owner;
  let addr1;
  let addr2;

  beforeEach(async function () {
    [owner, addr1, addr2] = await ethers.getSigners();

    const MyToken = await ethers.getContractFactory("MyToken");
    myToken = await MyToken.deploy();
    await myToken.waitForDeployment();
  });

  describe("Deployment", function () {
    it("Should set the right owner", async function () {
      expect(await myToken.owner()).to.equal(owner.address);
    });

    it("Should assign the initial supply to the owner", async function () {
      const ownerBalance = await myToken.balanceOf(owner.address);
      expect(await myToken.totalSupply()).to.equal(ownerBalance);
    });

    it("Should have correct name and symbol", async function () {
      expect(await myToken.name()).to.equal("MyToken");
      expect(await myToken.symbol()).to.equal("MTK");
    });
  });

  describe("Minting", function () {
    it("Should mint tokens to address", async function () {
      const mintAmount = ethers.parseUnits("1000", 18);
      await myToken.mint(addr1.address, mintAmount);

      const addr1Balance = await myToken.balanceOf(addr1.address);
      expect(addr1Balance).to.equal(mintAmount);
    });

    it("Should fail if non-owner tries to mint", async function () {
      const mintAmount = ethers.parseUnits("1000", 18);

      await expect(
        myToken.connect(addr1).mint(addr2.address, mintAmount)
      ).to.be.revertedWithCustomError(myToken, "OwnableUnauthorizedAccount");
    });

    it("Should not exceed max supply", async function () {
      const maxSupply = await myToken.MAX_SUPPLY();
      const currentSupply = await myToken.totalSupply();
      const remainingSupply = maxSupply - currentSupply;

      await expect(
        myToken.mint(addr1.address, remainingSupply + 1n)
      ).to.be.revertedWith("Exceeds max supply");
    });
  });

  describe("Burning", function () {
    it("Should burn tokens from caller's balance", async function () {
      const burnAmount = ethers.parseUnits("1000", 18);
      const initialBalance = await myToken.balanceOf(owner.address);

      await myToken.burn(burnAmount);

      const finalBalance = await myToken.balanceOf(owner.address);
      expect(finalBalance).to.equal(initialBalance - burnAmount);
    });

    it("Should decrease total supply when burning", async function () {
      const burnAmount = ethers.parseUnits("1000", 18);
      const initialSupply = await myToken.totalSupply();

      await myToken.burn(burnAmount);

      const finalSupply = await myToken.totalSupply();
      expect(finalSupply).to.equal(initialSupply - burnAmount);
    });
  });

  describe("Transfers", function () {
    it("Should transfer tokens between accounts", async function () {
      const transferAmount = ethers.parseUnits("50", 18);

      await myToken.transfer(addr1.address, transferAmount);
      const addr1Balance = await myToken.balanceOf(addr1.address);
      expect(addr1Balance).to.equal(transferAmount);

      await myToken.connect(addr1).transfer(addr2.address, transferAmount);
      const addr2Balance = await myToken.balanceOf(addr2.address);
      expect(addr2Balance).to.equal(transferAmount);
    });

    it("Should fail if sender doesn't have enough tokens", async function () {
      const initialOwnerBalance = await myToken.balanceOf(owner.address);

      await expect(
        myToken.connect(addr1).transfer(owner.address, 1)
      ).to.be.revertedWithCustomError(myToken, "ERC20InsufficientBalance");

      expect(await myToken.balanceOf(owner.address)).to.equal(
        initialOwnerBalance
      );
    });
  });
});
