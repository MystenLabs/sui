const { expect } = require("chai");
const { ethers } = require("hardhat");

const make_test = function (contract_name) {
  return function () {
    before(async function () {
      this.Event = await ethers.getContractFactory(contract_name);
      this.event = await this.Event.deploy();
      await this.event.deployed();
    });
    it("emitSimpleEvent(42) should emit an event", async function () {
      const tx = this.event.emitSimpleEvent(42);
      await expect(tx).to.emit(this.event, 'SimpleEvent').withArgs(42);
    });
    it("emitSimpleEventTwice(42) should emit two events", async function () {
      const tx = this.event.emitSimpleEventTwice(42);
      await expect(tx).to.emit(this.event, 'SimpleEvent').withArgs(42);
      await expect(tx).to.emit(this.event, 'SimpleEvent').withArgs(84);
    });
    it("emitMyEvent(42) should emit an event", async function () {
      const tx = this.event.emitMyEvent(42);
      await expect(tx).to.emit(this.event, 'MyEvent').withArgs(42, 'hello_event');
    });
    it("emitMyEventTwice(42) should emit two events", async function () {
      const tx = this.event.emitMyEventTwice(42);
      await expect(tx).to.emit(this.event, 'MyEvent').withArgs(42, 'hello_event_#1');
      await expect(tx).to.emit(this.event, 'MyEvent').withArgs(84, 'hello_event_#2');
    });
    it("emitMyEventWith(42, 'hello_event') should emit an event", async function () {
      const tx = this.event.emitMyEventWith(42, "hello_event");
      await expect(tx).to.emit(this.event, 'MyEvent').withArgs(42, 'hello_event');
    });
    it("emitMyEventWithTwice(42, 'hello_event') should emit two events", async function () {
      const tx = this.event.emitMyEventWithTwice(42, "hello_event");
      await expect(tx).to.emit(this.event, 'MyEvent').withArgs(42, 'hello_event');
      await expect(tx).to.emit(this.event, 'MyEvent').withArgs(84, 'hello_event');
    });
    it("emitU256Event should emit a U256Event event", async function () {
      const tx = this.event.emitU256Event(0);
      await expect(tx).to.emit(this.event, 'U256Event').withArgs(0);
    });
    it("emitAddressEvent should emit a AddressEvent", async function () {
      const [owner] = await ethers.getSigners();
      const tx = this.event.emitAddressEvent(owner.address);
      await expect(tx).to.emit(this.event, 'AddressEvent').withArgs(owner.address);
    });
    it("emitTransfer should emit a Transfer event", async function () {
      const [owner] = await ethers.getSigners();
      const tx = this.event.emitTransfer(owner.address, owner.address, 0);
      await expect(tx).to.emit(this.event, 'Transfer').withArgs(owner.address, owner.address, 0);
    });
  }
};

describe("Event (the Move contract)", make_test('Event'));
describe("Event_Sol (the Solidity contract)", make_test('Event_Sol'));
