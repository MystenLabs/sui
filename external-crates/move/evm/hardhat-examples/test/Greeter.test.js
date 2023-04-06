const { expect } = require("chai");
const { ethers } = require("hardhat");


const make_test = function (contract_name) {
  return function () {
    it("Should return the new greeting once it's changed", async function () {
      const Greeter = await ethers.getContractFactory(contract_name);
      const greeter = await Greeter.deploy("Hello, world!");
      await greeter.deployed();
      expect(await greeter.greet()).to.equal("Hello, world!");
      const setGreetingTx = await greeter.setGreeting("Hola, mundo!");
      // wait until the transaction is mined
      await setGreetingTx.wait();
      expect(await greeter.greet()).to.equal("Hola, mundo!");
    });
  }
};

describe("Greeter (the Move contract)", make_test("Greeter"));
describe("Greeter_Sol (the Solidity Contract)", make_test("Greeter_Sol"));
