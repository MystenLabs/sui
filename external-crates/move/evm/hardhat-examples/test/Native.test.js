const { expect } = require("chai");
const { ethers } = require("hardhat");

const make_test = function(contract_name) {
  return function () {
    before(async function () {
      this.Native = await ethers.getContractFactory(contract_name);
      this.native = await this.Native.deploy();
      await this.native.deployed();
    });
    it("getContractAddr() should return the contract address", async function () {
      const tx = this.native.getContractAddr();
      expect(await tx).to.equal(this.native.address);
    });
    it("getSenderAddr() should return the sender address", async function () {
      const tx = this.native.getSenderAddr();
      expect(await tx).to.equal(this.native.signer.address);
    });
    it("getIsContract(this.native.address) should return true", async function () {
      const tx = this.native.getIsContract(this.native.address);
      expect(await tx).to.equal(true);
    });
    it("getIsContract(this.native.signer.address) should return false", async function () {
      const tx = this.native.getIsContract(this.native.signer.address);
      expect(await tx).to.equal(false);
    });
  }
};

describe("Native (the Move contract)", make_test("Native"));
describe("Native_Sol (the Solidity contract)", make_test("Native_Sol"));
