const { expect } = require("chai");
const { ethers } = require("hardhat");

const make_test = function (contract_name) {
  return function () {
    before(async function () {
      this.Revert = await ethers.getContractFactory(contract_name);
      this.revert = await this.Revert.deploy();
      await this.revert.deployed();
    });
    it("revertIf0(0) should revert", async function () {
      const tx = this.revert.revertIf0(0);
      await expect(tx).to.be.reverted;
    });
    // TODO: Support reverting with an error message
    // it("revertWithMessage() should revert with a message", async function () {
    //   const tx = this.revert.revertWithMessage();
    //   await expect(tx).to.be.revertedWith('error message');
    // });
  }
};

describe("Revert (the Move contract)", make_test("Revert"));
describe("Revert_Sol (the Solidity contract)", make_test("Revert_Sol"));
