export const mapDatafromApi = (data) => {
  return data.map(item => {
      return {
          claim_status: item.claim_status,
          name: item.token.name || false,
          media_uri: item.token.media_uri ? item.token.media_uri.replace('ipfs://', 'https://ipfs.io/ipfs/') : false,
          token_id: item.token.token_id,
          contract_address: item.token.contract_address,
          noMedia: item.token.media_uri ? false : true

      }
  }).filter(item => item.media_uri && item.name)
}