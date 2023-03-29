const { BN, constants, expectEvent, expectRevert } = require('@openzeppelin/test-helpers');
const { expect } = require('chai');
const { ZERO_ADDRESS } = constants;

const Event = artifacts.require('Event');

contract('Truffle-style testing for Event (the Move contract)', function (accounts) {
    beforeEach(async function () {
        this.event = await Event.new();
    });
    it('emits an SimpleEvent', async function () {
        expectEvent(
            await this.event.emitSimpleEvent(42),
            'SimpleEvent',
            [new BN(42)],
        );
    });
    it('emits an MyEvent', async function () {
        expectEvent(
            await this.event.emitMyEvent(7),
            'MyEvent',
            [new BN(7)],
        );
    });
    it('emits an Transfer event', async function () {
        expectEvent(
            await this.event.emitTransfer(ZERO_ADDRESS, ZERO_ADDRESS, 7),
            'Transfer',
            [ZERO_ADDRESS, ZERO_ADDRESS, new BN(7)],
        );
    });
    // Enable this to show the events emitted.
    // it('display the events emitted', async function () {
    //     await this.event.emitTransfer(ZERO_ADDRESS, ZERO_ADDRESS, new BN(7));
    //     let events = await this.event.getPastEvents();
    //     console.log(events);
    // });
});
