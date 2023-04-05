const { BN, constants, expectEvent, expectRevert } = require('@openzeppelin/test-helpers');
const { expect } = require("chai");
const { ethers } = require("hardhat");

const make_test = function (caller_contract, callee_contract) {
  return function () {
    it("Call", async function () {
      this.Caller = await ethers.getContractFactory(caller_contract);
      this.Callee = await ethers.getContractFactory(callee_contract);
      this.caller = await this.Caller.deploy();
      this.callee = await this.Callee.deploy();
      const tx = this.caller.call_success(this.callee.address);
      await expect(await tx).to.be.equal(42);
      const tx_2 = this.caller.call_revert(this.callee.address);
      await expect(await tx_2).to.be.equal("revert");
      const tx_3 = this.caller.call_panic(this.callee.address);
      await expect(await tx_3).to.be.equal(0x12);
    });
  }
};

describe("Caller (the Move contract)", make_test("Caller", "Callee"));
