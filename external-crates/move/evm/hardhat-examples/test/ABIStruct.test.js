const { BN, constants, expectEvent, expectRevert } = require('@openzeppelin/test-helpers');
const { expect } = require("chai");
const { ethers } = require("hardhat");

const make_test = function (move_contract, sol_contract) {
  return function () {
    it("CallMove", async function () {
      this.ABIStruct = await ethers.getContractFactory(move_contract);
      this.StructABI_Sol = await ethers.getContractFactory(sol_contract);
      this.abi_struct = await this.ABIStruct.deploy();
      this.struct_abi_sol = await this.StructABI_Sol.deploy(this.abi_struct.address);
      const tx = this.struct_abi_sol.call_s();
      await expect(await tx).to.emit(this.abi_struct, 'Event_u64').withArgs(42);
    });

    it("CallSol", async function () {
      this.ABIStruct = await ethers.getContractFactory(move_contract);
      this.StructABI_Sol = await ethers.getContractFactory(sol_contract);
      this.abi_struct = await this.ABIStruct.deploy();
      this.struct_abi_sol = await this.StructABI_Sol.deploy(this.abi_struct.address);
      const tx = this.abi_struct.safe_transfer_form(this.struct_abi_sol.address);
      await expect(await tx).to.emit(this.abi_struct, 'Event_u64').withArgs(100);
    });

    it("StructEvent", async function () {
      const ABIStruct = artifacts.require('ABIStruct');
      this.abi_struct = await ABIStruct.new();
      expectEvent(await this.abi_struct.do_transfer(), 'Event_S', [["42", true, ["42"]]]);
    });

    it("StringEvent", async function () {
      const ABIStruct = artifacts.require('ABIStruct');
      this.abi_struct = await ABIStruct.new();
      expectEvent(await this.abi_struct.test_string([[97, 98, 99]]), 'Event_String', [["0x616263"]]);
    });

  }
};

describe("Caller (the Move contract)", make_test("ABIStruct", "StructABI_Sol"));
