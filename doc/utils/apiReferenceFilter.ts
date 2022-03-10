import apiReferenceData from '~/data/apiReference';

/// placeholder for api reference data

const swaggarApiReferenceUrl  = "https://app.swaggerhub.com/apis/MystenLabs/sui-api/0.1"
// 'https://app.swaggerhub.com/apis/MystenLabs/sui-api/0.1#/';

export const apiReferenceService = () => {
  // const paths = apiReferenceData.paths

  /// return all schemas list for now
  const info = apiReferenceData.info
  const schema:any = apiReferenceData.components.schemas

  const schemaData = Object.keys(schema).map((key:any) => {

    const properties = schema[key].properties
    const subMenu = Object.keys(properties).map((subKey) => {
      return {
        name: subKey,
        link: `${swaggarApiReferenceUrl}#/${key}`,
      }
    })
    return {
      name: key,
      link: `${swaggarApiReferenceUrl}#/${key}`,
      subMenu,
    }
  })
  return {
    menu : schemaData,
    info: info.version,
  }
}

// export default apiReferenceService;
