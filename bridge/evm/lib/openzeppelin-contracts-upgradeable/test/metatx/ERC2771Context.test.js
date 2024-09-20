const ethSigUtil = require('eth-sig-util');
const Wallet = require('ethereumjs-wallet').default;
const { getDomain, domainType } = require('../helpers/eip712');
const { MAX_UINT48 } = require('../helpers/constants');

const { expectEvent } = require('@openzeppelin/test-helpers');
const { expect } = require('chai');

const ERC2771ContextMock = artifacts.require('ERC2771ContextMock');
const ERC2771Forwarder = artifacts.require('ERC2771Forwarder');
const ContextMockCaller = artifacts.require('ContextMockCaller');

const { shouldBehaveLikeRegularContext } = require('../utils/Context.behavior');

contract('ERC2771Context', function (accounts) {
  const [, trustedForwarder, other] = accounts;

  beforeEach(async function () {
    this.forwarder = await ERC2771Forwarder.new('ERC2771Forwarder');
    this.recipient = await ERC2771ContextMock.new(this.forwarder.address);

    this.domain = await getDomain(this.forwarder);
    this.types = {
      EIP712Domain: domainType(this.domain),
      ForwardRequest: [
        { name: 'from', type: 'address' },
        { name: 'to', type: 'address' },
        { name: 'value', type: 'uint256' },
        { name: 'gas', type: 'uint256' },
        { name: 'nonce', type: 'uint256' },
        { name: 'deadline', type: 'uint48' },
        { name: 'data', type: 'bytes' },
      ],
    };
  });

  it('recognize trusted forwarder', async function () {
    expect(await this.recipient.isTrustedForwarder(this.forwarder.address)).to.equal(true);
  });

  it('returns the trusted forwarder', async function () {
    expect(await this.recipient.trustedForwarder()).to.equal(this.forwarder.address);
  });

  context('when called directly', function () {
    beforeEach(async function () {
      this.context = this.recipient; // The Context behavior expects the contract in this.context
      this.caller = await ContextMockCaller.new();
    });

    shouldBehaveLikeRegularContext(...accounts);
  });

  context('when receiving a relayed call', function () {
    beforeEach(async function () {
      this.wallet = Wallet.generate();
      this.sender = web3.utils.toChecksumAddress(this.wallet.getAddressString());
      this.data = {
        types: this.types,
        domain: this.domain,
        primaryType: 'ForwardRequest',
      };
    });

    describe('msgSender', function () {
      it('returns the relayed transaction original sender', async function () {
        const data = this.recipient.contract.methods.msgSender().encodeABI();

        const req = {
          from: this.sender,
          to: this.recipient.address,
          value: '0',
          gas: '100000',
          nonce: (await this.forwarder.nonces(this.sender)).toString(),
          deadline: MAX_UINT48,
          data,
        };

        req.signature = ethSigUtil.signTypedMessage(this.wallet.getPrivateKey(), {
          data: { ...this.data, message: req },
        });
        expect(await this.forwarder.verify(req)).to.equal(true);

        const { tx } = await this.forwarder.execute(req);
        await expectEvent.inTransaction(tx, ERC2771ContextMock, 'Sender', { sender: this.sender });
      });

      it('returns the original sender when calldata length is less than 20 bytes (address length)', async function () {
        // The forwarder doesn't produce calls with calldata length less than 20 bytes
        const recipient = await ERC2771ContextMock.new(trustedForwarder);

        const { receipt } = await recipient.msgSender({ from: trustedForwarder });

        await expectEvent(receipt, 'Sender', { sender: trustedForwarder });
      });
    });

    describe('msgData', function () {
      it('returns the relayed transaction original data', async function () {
        const integerValue = '42';
        const stringValue = 'OpenZeppelin';
        const data = this.recipient.contract.methods.msgData(integerValue, stringValue).encodeABI();

        const req = {
          from: this.sender,
          to: this.recipient.address,
          value: '0',
          gas: '100000',
          nonce: (await this.forwarder.nonces(this.sender)).toString(),
          deadline: MAX_UINT48,
          data,
        };

        req.signature = ethSigUtil.signTypedMessage(this.wallet.getPrivateKey(), {
          data: { ...this.data, message: req },
        });
        expect(await this.forwarder.verify(req)).to.equal(true);

        const { tx } = await this.forwarder.execute(req);
        await expectEvent.inTransaction(tx, ERC2771ContextMock, 'Data', { data, integerValue, stringValue });
      });
    });

    it('returns the full original data when calldata length is less than 20 bytes (address length)', async function () {
      // The forwarder doesn't produce calls with calldata length less than 20 bytes
      const recipient = await ERC2771ContextMock.new(trustedForwarder);

      const { receipt } = await recipient.msgDataShort({ from: trustedForwarder });

      const data = recipient.contract.methods.msgDataShort().encodeABI();
      await expectEvent(receipt, 'DataShort', { data });
    });
  });

  it('multicall poison attack', async function () {
    const attacker = Wallet.generate();
    const attackerAddress = attacker.getChecksumAddressString();
    const nonce = await this.forwarder.nonces(attackerAddress);

    const msgSenderCall = web3.eth.abi.encodeFunctionCall(
      {
        name: 'msgSender',
        type: 'function',
        inputs: [],
      },
      [],
    );

    const data = web3.eth.abi.encodeFunctionCall(
      {
        name: 'multicall',
        type: 'function',
        inputs: [
          {
            internalType: 'bytes[]',
            name: 'data',
            type: 'bytes[]',
          },
        ],
      },
      [[web3.utils.encodePacked({ value: msgSenderCall, type: 'bytes' }, { value: other, type: 'address' })]],
    );

    const req = {
      from: attackerAddress,
      to: this.recipient.address,
      value: '0',
      gas: '100000',
      data,
      nonce: Number(nonce),
      deadline: MAX_UINT48,
    };

    req.signature = await ethSigUtil.signTypedMessage(attacker.getPrivateKey(), {
      data: {
        types: this.types,
        domain: this.domain,
        primaryType: 'ForwardRequest',
        message: req,
      },
    });

    expect(await this.forwarder.verify(req)).to.equal(true);

    const receipt = await this.forwarder.execute(req);
    await expectEvent.inTransaction(receipt.tx, ERC2771ContextMock, 'Sender', { sender: attackerAddress });
  });
});
