const { expect } = require("chai");
const { ethers } = require("hardhat");

const make_test = function (contract_name) {
  return function () {
    before(async function () {
      this.Event = await ethers.getContractFactory(contract_name);
      this.event = await this.Event.deploy();
      await this.event.deployed();
    });
    it("AsyncEvent_xfer_deposit", async function () {
      const tx = this.event.xfer(this.event.address,42);
      await expect(tx).to.emit(this.event, 'Xfer_deposit').withArgs(this.event.address, 0x0b32e0fa, 42, this.event.address, 0);
    });
    it("AsyncEvent_xfer_finish", async function () {
      const tx = this.event.xfer_deposit(42,this.event.address,1);
      await expect(tx).to.emit(this.event, 'Xfer_finish').withArgs(this.event.address, 0xb8229d65, 1);
    });
  }
};

describe("Async Event (the Async Move contract)", make_test('AsyncEvent'));
