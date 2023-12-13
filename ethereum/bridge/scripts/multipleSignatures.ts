import { ethers } from 'ethers'
;(async () => {
    try {
        for (let i = 0; i < 20; i++) {
        let wallet = ethers.Wallet.createRandom();

        const messageType = 2
        const messageVersion = 1
        const sequenceNumber = 0
        const sourceChain = 0
        const payload = "0x00"

        const messageHash = ethers.solidityPackedKeccak256(
            ['uint8', 'uint8', 'uint64', 'uint8', 'bytes'],
            [messageType, messageVersion, sequenceNumber, sourceChain, payload],
        )
        const messageHashBinary = ethers.getBytes(messageHash)
        const signature = await wallet.signMessage(messageHashBinary)

        console.log(
            `messageHash:"${messageHash}"\nmessageHashBinary:${messageHashBinary}`,
        )
        console.log(
            `signature:"${signature}"\nsigner:${await wallet.getAddress()}`,
        )
    }
    } catch (e: any) {
        console.log(e.message)
    }
})()
