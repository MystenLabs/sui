export const mapDatafromApi = (data) => {
  // return empty array if data is not available
  if (!data) return [];

  /// remove this later
  const nftTxt = '0xbc4ca0eda7647a8ab7c2061c2e118a18a936f13d'
  const nFTname = "Bored Ape Yacht Club #"
  const replaceTokenId = (str:string) =>{
    return str  === '0x00000000000000000000000000000000000000000000000000000000000022e9' ? '8937' : (str  === '0x00000000000000000000000000000000000000000000000000000000000022ae' ? '8878': str) 
  }
  return data.map(item => {
      return {
          claim_status: item.claim_status,
          name: item.token.contract_address === nftTxt ? nFTname + replaceTokenId(item.token.token_id) :  (item.token.name || false) ,
          media_uri: item.token.media_uri ? item.token.media_uri.replace('ipfs://', 'https://ipfs.io/ipfs/') : false,
          token_id: item.token.token_id,
          contract_address: item.token.contract_address,
          noMedia: item.token.media_uri ? false : true

      }
  }).filter(item => item.media_uri && item.name)
}