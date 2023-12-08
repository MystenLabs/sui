import { ethers } from 'ethers'
;(async () => {
    try {
        const privateKey =
            '0x3f614a2b69459e93371336703571a74af91cab7ba05fd56a66b55eb1ba24ce55'
        let wallet = new ethers.Wallet(privateKey)

        const messageType = 2
        const version = 0
        const sourceChain = 0
        const bridgeSeqNum = 0
        const senderAddress = '0x5567f54B29B973343d632f7BFCe9507343D41FCa'
        const targetChain = 1
        const targetAddress = '0x5567f54B29B973343d632f7BFCe9507343D41FCa'

        const messageHash = ethers.solidityPackedKeccak256(
            ['uint8', 'uint8', 'uint8', 'uint64', 'address', 'uint8' ,'address'],
            [messageType, version, sourceChain, bridgeSeqNum, senderAddress, targetChain, targetAddress],
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
