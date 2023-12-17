import { ethers } from 'ethers'
    ; (async () => {
        try {
            const privateKey =
                '0x3f614a2b69459e93371336703571a74af91cab7ba05fd56a66b55eb1ba24ce55'
            let wallet = new ethers.Wallet(privateKey)

            const messageType = 0;
            const messageVersion = 0;
            const nonce = 0;
            const sourceChain = 0;
            const sourceChainTxIdLength = 0;
            const sourceChainTxId = '0x00';
            const sourceChainEventIndex = 0;
            const senderAddressLength = 0;
            const senderAddress = '0x00';
            const targetChain = 0;
            const targetChainLength = 0;
            const targetAddress = '0x00';
            const tokenType = 0;
            const amount = 1000;

            const messageHash = ethers.solidityPackedKeccak256(
                ['uint8', 'uint8', 'uint64', 'uint8', 'uint8', 'bytes', 'uint8', 'uint8', 'bytes', 'uint8', 'uint8', 'bytes', 'uint8', 'uint64'],
                [messageType, messageVersion, nonce, sourceChain, sourceChainTxIdLength, sourceChainTxId, sourceChainEventIndex, senderAddressLength, senderAddress, targetChain, targetChainLength, targetAddress, tokenType, amount],
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
