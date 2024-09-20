require('@openzeppelin/test-helpers');
const { expectRevertCustomError } = require('../../helpers/customError');
const { toEthSignedMessageHash } = require('../../helpers/sign');

const { expect } = require('chai');

const ECDSA = artifacts.require('$ECDSA');

const TEST_MESSAGE = web3.utils.sha3('OpenZeppelin');
const WRONG_MESSAGE = web3.utils.sha3('Nope');
const NON_HASH_MESSAGE = '0x' + Buffer.from('abcd').toString('hex');

function to2098Format(signature) {
  const long = web3.utils.hexToBytes(signature);
  if (long.length !== 65) {
    throw new Error('invalid signature length (expected long format)');
  }
  if (long[32] >> 7 === 1) {
    throw new Error("invalid signature 's' value");
  }
  const short = long.slice(0, 64);
  short[32] |= long[64] % 27 << 7; // set the first bit of the 32nd byte to the v parity bit
  return web3.utils.bytesToHex(short);
}

function split(signature) {
  const raw = web3.utils.hexToBytes(signature);
  switch (raw.length) {
    case 64:
      return [
        web3.utils.bytesToHex(raw.slice(0, 32)), // r
        web3.utils.bytesToHex(raw.slice(32, 64)), // vs
      ];
    case 65:
      return [
        raw[64], // v
        web3.utils.bytesToHex(raw.slice(0, 32)), // r
        web3.utils.bytesToHex(raw.slice(32, 64)), // s
      ];
    default:
      expect.fail('Invalid signature length, cannot split');
  }
}

contract('ECDSA', function (accounts) {
  const [other] = accounts;

  beforeEach(async function () {
    this.ecdsa = await ECDSA.new();
  });

  context('recover with invalid signature', function () {
    it('with short signature', async function () {
      await expectRevertCustomError(this.ecdsa.$recover(TEST_MESSAGE, '0x1234'), 'ECDSAInvalidSignatureLength', [2]);
    });

    it('with long signature', async function () {
      await expectRevertCustomError(
        // eslint-disable-next-line max-len
        this.ecdsa.$recover(
          TEST_MESSAGE,
          '0x01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789',
        ),
        'ECDSAInvalidSignatureLength',
        [85],
      );
    });
  });

  context('recover with valid signature', function () {
    context('using web3.eth.sign', function () {
      it('returns signer address with correct signature', async function () {
        // Create the signature
        const signature = await web3.eth.sign(TEST_MESSAGE, other);

        // Recover the signer address from the generated message and signature.
        expect(await this.ecdsa.$recover(toEthSignedMessageHash(TEST_MESSAGE), signature)).to.equal(other);
      });

      it('returns signer address with correct signature for arbitrary length message', async function () {
        // Create the signature
        const signature = await web3.eth.sign(NON_HASH_MESSAGE, other);

        // Recover the signer address from the generated message and signature.
        expect(await this.ecdsa.$recover(toEthSignedMessageHash(NON_HASH_MESSAGE), signature)).to.equal(other);
      });

      it('returns a different address', async function () {
        const signature = await web3.eth.sign(TEST_MESSAGE, other);
        expect(await this.ecdsa.$recover(WRONG_MESSAGE, signature)).to.not.equal(other);
      });

      it('reverts with invalid signature', async function () {
        // eslint-disable-next-line max-len
        const signature =
          '0x332ce75a821c982f9127538858900d87d3ec1f9f737338ad67cad133fa48feff48e6fa0c18abc62e42820f05943e47af3e9fbe306ce74d64094bdf1691ee53e01c';
        await expectRevertCustomError(this.ecdsa.$recover(TEST_MESSAGE, signature), 'ECDSAInvalidSignature', []);
      });
    });

    context('with v=27 signature', function () {
      // Signature generated outside ganache with method web3.eth.sign(signer, message)
      const signer = '0x2cc1166f6212628A0deEf2B33BEFB2187D35b86c';
      // eslint-disable-next-line max-len
      const signatureWithoutV =
        '0x5d99b6f7f6d1f73d1a26497f2b1c89b24c0993913f86e9a2d02cd69887d9c94f3c880358579d811b21dd1b7fd9bb01c1d81d10e69f0384e675c32b39643be892';

      it('works with correct v value', async function () {
        const v = '1b'; // 27 = 1b.
        const signature = signatureWithoutV + v;
        expect(await this.ecdsa.$recover(TEST_MESSAGE, signature)).to.equal(signer);

        expect(
          await this.ecdsa.methods['$recover(bytes32,uint8,bytes32,bytes32)'](TEST_MESSAGE, ...split(signature)),
        ).to.equal(signer);

        expect(
          await this.ecdsa.methods['$recover(bytes32,bytes32,bytes32)'](
            TEST_MESSAGE,
            ...split(to2098Format(signature)),
          ),
        ).to.equal(signer);
      });

      it('rejects incorrect v value', async function () {
        const v = '1c'; // 28 = 1c.
        const signature = signatureWithoutV + v;
        expect(await this.ecdsa.$recover(TEST_MESSAGE, signature)).to.not.equal(signer);

        expect(
          await this.ecdsa.methods['$recover(bytes32,uint8,bytes32,bytes32)'](TEST_MESSAGE, ...split(signature)),
        ).to.not.equal(signer);

        expect(
          await this.ecdsa.methods['$recover(bytes32,bytes32,bytes32)'](
            TEST_MESSAGE,
            ...split(to2098Format(signature)),
          ),
        ).to.not.equal(signer);
      });

      it('reverts wrong v values', async function () {
        for (const v of ['00', '01']) {
          const signature = signatureWithoutV + v;
          await expectRevertCustomError(this.ecdsa.$recover(TEST_MESSAGE, signature), 'ECDSAInvalidSignature', []);

          await expectRevertCustomError(
            this.ecdsa.methods['$recover(bytes32,uint8,bytes32,bytes32)'](TEST_MESSAGE, ...split(signature)),
            'ECDSAInvalidSignature',
            [],
          );
        }
      });

      it('rejects short EIP2098 format', async function () {
        const v = '1b'; // 27 = 1b.
        const signature = signatureWithoutV + v;
        await expectRevertCustomError(
          this.ecdsa.$recover(TEST_MESSAGE, to2098Format(signature)),
          'ECDSAInvalidSignatureLength',
          [64],
        );
      });
    });

    context('with v=28 signature', function () {
      const signer = '0x1E318623aB09Fe6de3C9b8672098464Aeda9100E';
      // eslint-disable-next-line max-len
      const signatureWithoutV =
        '0x331fe75a821c982f9127538858900d87d3ec1f9f737338ad67cad133fa48feff48e6fa0c18abc62e42820f05943e47af3e9fbe306ce74d64094bdf1691ee53e0';

      it('works with correct v value', async function () {
        const v = '1c'; // 28 = 1c.
        const signature = signatureWithoutV + v;
        expect(await this.ecdsa.$recover(TEST_MESSAGE, signature)).to.equal(signer);

        expect(
          await this.ecdsa.methods['$recover(bytes32,uint8,bytes32,bytes32)'](TEST_MESSAGE, ...split(signature)),
        ).to.equal(signer);

        expect(
          await this.ecdsa.methods['$recover(bytes32,bytes32,bytes32)'](
            TEST_MESSAGE,
            ...split(to2098Format(signature)),
          ),
        ).to.equal(signer);
      });

      it('rejects incorrect v value', async function () {
        const v = '1b'; // 27 = 1b.
        const signature = signatureWithoutV + v;
        expect(await this.ecdsa.$recover(TEST_MESSAGE, signature)).to.not.equal(signer);

        expect(
          await this.ecdsa.methods['$recover(bytes32,uint8,bytes32,bytes32)'](TEST_MESSAGE, ...split(signature)),
        ).to.not.equal(signer);

        expect(
          await this.ecdsa.methods['$recover(bytes32,bytes32,bytes32)'](
            TEST_MESSAGE,
            ...split(to2098Format(signature)),
          ),
        ).to.not.equal(signer);
      });

      it('reverts invalid v values', async function () {
        for (const v of ['00', '01']) {
          const signature = signatureWithoutV + v;
          await expectRevertCustomError(this.ecdsa.$recover(TEST_MESSAGE, signature), 'ECDSAInvalidSignature', []);

          await expectRevertCustomError(
            this.ecdsa.methods['$recover(bytes32,uint8,bytes32,bytes32)'](TEST_MESSAGE, ...split(signature)),
            'ECDSAInvalidSignature',
            [],
          );
        }
      });

      it('rejects short EIP2098 format', async function () {
        const v = '1c'; // 27 = 1b.
        const signature = signatureWithoutV + v;
        await expectRevertCustomError(
          this.ecdsa.$recover(TEST_MESSAGE, to2098Format(signature)),
          'ECDSAInvalidSignatureLength',
          [64],
        );
      });
    });

    it('reverts with high-s value signature', async function () {
      const message = '0xb94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9';
      // eslint-disable-next-line max-len
      const highSSignature =
        '0xe742ff452d41413616a5bf43fe15dd88294e983d3d36206c2712f39083d638bde0a0fc89be718fbc1033e1d30d78be1c68081562ed2e97af876f286f3453231d1b';
      const [r, v, s] = split(highSSignature);
      await expectRevertCustomError(this.ecdsa.$recover(message, highSSignature), 'ECDSAInvalidSignatureS', [s]);
      await expectRevertCustomError(
        this.ecdsa.methods['$recover(bytes32,uint8,bytes32,bytes32)'](TEST_MESSAGE, r, v, s),
        'ECDSAInvalidSignatureS',
        [s],
      );
      expect(() => to2098Format(highSSignature)).to.throw("invalid signature 's' value");
    });
  });
});
