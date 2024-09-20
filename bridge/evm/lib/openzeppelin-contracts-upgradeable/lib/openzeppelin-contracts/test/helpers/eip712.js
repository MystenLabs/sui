const ethSigUtil = require('eth-sig-util');
const keccak256 = require('keccak256');

const EIP712Domain = [
  { name: 'name', type: 'string' },
  { name: 'version', type: 'string' },
  { name: 'chainId', type: 'uint256' },
  { name: 'verifyingContract', type: 'address' },
  { name: 'salt', type: 'bytes32' },
];

const Permit = [
  { name: 'owner', type: 'address' },
  { name: 'spender', type: 'address' },
  { name: 'value', type: 'uint256' },
  { name: 'nonce', type: 'uint256' },
  { name: 'deadline', type: 'uint256' },
];

function bufferToHexString(buffer) {
  return '0x' + buffer.toString('hex');
}

function hexStringToBuffer(hexstr) {
  return Buffer.from(hexstr.replace(/^0x/, ''), 'hex');
}

async function getDomain(contract) {
  const { fields, name, version, chainId, verifyingContract, salt, extensions } = await contract.eip712Domain();

  if (extensions.length > 0) {
    throw Error('Extensions not implemented');
  }

  const domain = { name, version, chainId, verifyingContract, salt };
  for (const [i, { name }] of EIP712Domain.entries()) {
    if (!(fields & (1 << i))) {
      delete domain[name];
    }
  }

  return domain;
}

function domainType(domain) {
  return EIP712Domain.filter(({ name }) => domain[name] !== undefined);
}

function domainSeparator(domain) {
  return bufferToHexString(
    ethSigUtil.TypedDataUtils.hashStruct('EIP712Domain', domain, { EIP712Domain: domainType(domain) }),
  );
}

function hashTypedData(domain, structHash) {
  return bufferToHexString(
    keccak256(Buffer.concat(['0x1901', domainSeparator(domain), structHash].map(str => hexStringToBuffer(str)))),
  );
}

module.exports = {
  Permit,
  getDomain,
  domainType,
  domainSeparator,
  hashTypedData,
};
