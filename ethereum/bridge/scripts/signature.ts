import { ethers } from 'ethers'
;(async () => {
    try {
        const privateKey =
            '0x3f614a2b69459e93371336703571a74af91cab7ba05fd56a66b55eb1ba24ce55'
        let wallet = new ethers.Wallet(privateKey)

        const messageType = 2
        const messageVersion = 1
        const sequenceNumber = 0
        const sourceChain = 0
        const payload = '0x00'

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
    } catch (e: any) {
        console.log(e.message)
    }
})()
