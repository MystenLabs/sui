# Sui Etkinlikleri API'ı

[Sui Tam node](https://docs.sui.io/devnet/build/fullnode)'ları, WebSocket API aracılığıyla [JSON-RPC](https://docs.sui.io/devnet/build/json-rpc) bildirimlerini kullanarak yayınlamayı / abone olmayı destekler. Move'dan veya Sui ağından oluşturulan gerçek zamanlı bir etkinlik akışını filtrelemek ve abone olmak için bu hizmeti Sui istemcisi ile kullanabilirsiniz.

İstemci, etkinliklerin kapsamını sınırlamak için bir [etkinlik filtresi](https://docs.sui.io/devnet/build/event\_api#event-filters) sağlar. Sui, filtreyle eşleşen her etkinlik için etkinlik verilerini ve abonelik kimliğini içeren bir bildirim döndürür.

### Etkinlik türleri

Bir Sui düğümü aşağıdaki etkinlik türlerini yayar:

* [Move Etkinliği](https://docs.sui.io/devnet/build/event\_api#move-event)
* [Etkinlik yayınlama](https://docs.sui.io/devnet/build/event\_api#publish-event)
* [Nesne transferi etkinliği](https://docs.sui.io/devnet/build/event\_api#transfer-object-event)
* [Nesne silme etkinliği](https://docs.sui.io/devnet/build/event\_api#delete-object-event)
* [Yeni nesne etkinliği](https://docs.sui.io/devnet/build/event\_api#new-object-event)
* [Dönem değişikliği etkinliği](https://docs.sui.io/devnet/build/event\_api#epoch-change-event)

### Move etkinliği <a href="#move-event" id="move-event"></a>

Move çağrıları Move etkinlikleri yayar. Move sözleşmelerinde [özel etkinlikler tanımlayabilirsiniz](https://examples.sui.io/basics/events.html).

### Özellikler

Etkinlik niteliklerini taşıyın:

* `packageId`
* `transactionModule`
* `sender`
* `type`
* `fields`
* `bcs`

#### Örnek Move Etkinliği <a href="#example-move-event" id="example-move-event"></a>

```
{
  "moveEvent": {
    "packageId": "0x0000000000000000000000000000000000000002",
    "transactionModule": "devnet_nft",
    "sender": "0x70613f4f17ae1363f7a7e7251daab5c5b06f68c1",
    "type": "0x2::devnet_nft::MintNFTEvent",
    "fields": {
      "creator": "0x70613f4f17ae1363f7a7e7251daab5c5b06f68c1",
      "name": "Example NFT",
      "object_id": "0x497913a47dc0028a85f24c70d825991b71c60001"
    },
    "bcs": "SXkTpH3AAoqF8kxw2CWZG3HGAAFwYT9PF64TY/en5yUdqrXFsG9owQtFeGFtcGxlIE5GVA=="
  }
}
```

### Etkinlik Yayımlama <a href="#publish-event" id="publish-event"></a>

Yayınlama etkinlikleri, bir paketi ağda yayınladığınızda gerçekleşir.

#### Özellikler <a href="#attributes-1" id="attributes-1"></a>

Etkinlik özellikleri yayınlayın:

* `sender`
* `packageId`

#### Örnek Etkinlik Yayımlama <a href="#example-publish-event" id="example-publish-event"></a>

```
{
  "publish": {
    "sender": "0x70613f4f17ae1363f7a7e7251daab5c5b06f68c1",
    "packageId": "0x2d052c9de3dd02f28ec0f8e4dfdee175a5c597c3"
  }
}
```

### Nesne Transferi Etkinliği <a href="#transfer-object-event" id="transfer-object-event"></a>

Nesne aktarma etkinlikleri, bir nesneyi bir adresten diğerine aktardığınızda gerçekleşir.

#### Özellikler <a href="#attributes-2" id="attributes-2"></a>

Transfer etkinliği özellikleri:

* `packageId`
* `transactionModule`
* `sender`
* `recipient`
* `objectId`
* `version`
* `type`

#### Nesne Transferi Etkinliği Örneği <a href="#example-transfer-object-event" id="example-transfer-object-event"></a>

```
{
  "transferObject": {
    "packageId": "0x0000000000000000000000000000000000000002",
    "transactionModule": "native",
    "sender": "0x70613f4f17ae1363f7a7e7251daab5c5b06f68c1",
    "recipient": {
      "AddressOwner": "0x741a9a7ea380aed286341fcf16176c8653feb667"
    },
    "objectId": "0x591fbb00a6c9676186cb44402040a8350520cbe9",
    "version": 1,
    "type": "Coin"
  }
}
```

### Nesne Silme Etkinliği <a href="#delete-object-event" id="delete-object-event"></a>

Nesne silme etkinlikleri, bir nesneyi sildiğinizde gerçekleşir.

#### Özellikleri <a href="#attributes-3" id="attributes-3"></a>

* `packageId`
* `transactionModule`
* `sender`
* `objectId`

### Örnek Nesne Silme Etkinliği

```
{
  "deleteObject": {
    "packageId": "0x2d052c9de3dd02f28ec0f8e4dfdee175a5c597c3",
    "transactionModule": "discount_coupon",
    "sender": "0x70613f4f17ae1363f7a7e7251daab5c5b06f68c1",
    "objectId": "0xe3a6bc7bf1dba4d17a91724009c461bd69870719"
  }
}
```

### Yeni nesne etkinliği

Ağ üzerinde bir nesne oluşturduğunuzda yeni nesne etkinlikleri gerçekleşir.

#### Özellikleri <a href="#attributes-4" id="attributes-4"></a>

Yeni nesne etkinliği özellikleri:

* `packageId`
* `transactionModule`
* `sender`
* `recipient`
* `objectId`

#### Yeni Nesne Etkinliği Örneği <a href="#example-new-object-event" id="example-new-object-event"></a>

```
{
  "newObject": {
    "packageId": "0x0000000000000000000000000000000000000002",
    "transactionModule": "devnet_nft",
    "sender": "0x70613f4f17ae1363f7a7e7251daab5c5b06f68c1",
    "recipient": {
      "AddressOwner": "0x70613f4f17ae1363f7a7e7251daab5c5b06f68c1"
    },
    "objectId": "0x497913a47dc0028a85f24c70d825991b71c60001"
  }
}
```

### Dönem değişikliği etkinliği

Dönem değişikliği etkinlikleri, bir dönem sona erdiğinde ve yeni bir dönem başladığında meydana gelir.

#### Özellikler <a href="#attributes-5" id="attributes-5"></a>

Yok, Dönem değişikliği etkinliklerinin herhangi bir özelliği yoktur. Etkinlik, epochChange ile ilişkili bir Epoch ID içerir.

#### Örnek Dönem değişikliği etkinliği

```
{
  "epochChange": 20
}
```

### Kontrol noktası etkinliği

Her kontrol noktası için bir kontrol noktası etkinliği gerçekleşir.

#### Özellikleri <a href="#attributes-6" id="attributes-6"></a>

Yok, Kontrol Noktası etkinliklerinin herhangi bir özelliği yoktur. Etkinlik, kontrol noktası ile ilişkili Kontrol Noktası sıra numarasını içerir.

#### Örnek Kontrol Noktası etkinliği

```
{
  "checkpoint": 10
}
```

### Sui etkinlik sorgu kriterleri

Bir Sui node'unu sorgulamak ve sorgu kriterleriyle eşleşen etkinlikleri almak için `EventQuery` kriter nesnesini kullanabilirsiniz.

| Sorgu       | Açıklama                                                                                                 | JSON-RPC Parametresi Örneği                                                  |
| ----------- | -------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------- |
| All         | Tüm etkinlikler                                                                                          | {"All"}                                                                      |
| Transaction | Belirtilen işlemden yayılan etkinlikler.                                                                 | {"Transaction":"DGUe2TXiJdN3FI6MH1FwghYbiHw+NKu8Nh579zdFtUk="}               |
| MoveModule  | Belirtilen Move modülünden yayılan etkinlikler                                                           | {"MoveModule":{"package":"0x2", "module":"devnet\_nft"\}}                    |
| MoveEvent   | Move etkinliğinin yapısal adı                                                                            | {"MoveEvent":"0x2::event\_nft::MintNFTEvent"}                                |
| EventType   | [Etkinlikler](https://docs.sui.io/devnet/build/event\_api#event-types) bölümünde açıklanan etkinlik türü | {"EventType": "NewObject"}                                                   |
| Sender      | Gönderen adresine göre sorgulama                                                                         | {"Sender":"0x70613f4f17ae1363f7a7e7251daab5c5b06f68c1"}                      |
| Recipient   | Alıcıya göre sorgulama                                                                                   | {"Recipient":{"AddressOwner":"0x70613f4f17ae1363f7a7e7251daab5c5b06f68c1"\}} |
| Object      | Verilen nesne ile ilişkili etkinlikleri döndürür                                                         | {"Object":"0xe3a6bc7bf1dba4d17a91724009c461bd69870719"}                      |
| TimeRange   | start\_time, end\_time] aralığında yayılan olayları döndürür                                             | {"TimeRange":{"startTime":1669039504014, "endTime":1669039604014\}}          |

Event Query API, büyük sonuç kümeleriyle çalışmayı kolaylaştırmak için imleç tabanlı sayfalandırma sağlar. Sorgunun başlangıç konumunu belirtmek için sayfalandırılmış sorguda bir `cursor` parametresi sağlayabilirsiniz. Sorgu, `limit` ile belirtilen sonuç sayısını döndürür ve ek sonuçlar olduğunda `next_cursor` değerini döndürür. Maksimum `limit` sorgu başına 1000'dir.

Aşağıdaki örnekler, sonuçlar için sayfalandırma kullanan sorguların nasıl oluşturulacağını göstermektedir.

#### 1. devnet\_nft modülü tarafından yayılan tüm olayları azalan zaman sırasına göre alın

İstek

```
curl --location --request POST '127.0.0.1:9000' \
--header 'Content-Type: application/json' \
--data-raw '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "sui_getEvents",
  "params": [
    {"MoveModule":{"package":"0x2", "module":"devnet_nft"}},
    null,
    null,
    true
  ]
}'
```

#### Yanıt

```
{
    "jsonrpc": "2.0",
    "result": {
        "data": [
            {
                "timestamp": 1666699837426,
                "txDigest": "cZXsToU6r0Uia6HIAwvr1eMlGsrg6b9+2oYZAskJ0wc=",
                "id": {
                    "txSeq": 1001,
                    "eventSeq": 1,
                },
                "event": {
                    "moveEvent": {
                        "packageId": "0x0000000000000000000000000000000000000002",
                        "transactionModule": "devnet_nft",
                        "sender": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                        "type": "0x2::devnet_nft::MintNFTEvent",
                        "fields": {
                            "creator": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                            "name": "Example NFT",
                            "object_id": "0x2ee80b4a2d203365dfbd68a90a8ad9a0dca19155"
                        },
                        "bcs": "LugLSi0gM2XfvWipCorZoNyhkVX+1JBtcbilg//9jpVnYCe2u4HXzwtFeGFtcGxlIE5GVA=="
                    }
                }
            },
            {
                "timestamp": 1666699837426,
                "txDigest": "cZXsToU6r0Uia6HIAwvr1eMlGsrg6b9+2oYZAskJ0wc=",
                "id": {
                    "txSeq": 1001,
                    "eventSeq": 0,
                },
                "event": {
                    "newObject": {
                        "packageId": "0x0000000000000000000000000000000000000002",
                        "transactionModule": "devnet_nft",
                        "sender": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                        "recipient": {
                            "AddressOwner": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf"
                        },
                        "objectId": "0x2ee80b4a2d203365dfbd68a90a8ad9a0dca19155"
                    }
                }
            },
            {
                "timestamp": 1666698739180,
                "txDigest": "WF2V6FM6y/kpAgRqzsQmR/osy4pmTgVVbE6qvSJxWh4=",
                "id": {
                    "txSeq": 998,
                    "eventSeq": 1,
                },
                "event": {
                    "moveEvent": {
                        "packageId": "0x0000000000000000000000000000000000000002",
                        "transactionModule": "devnet_nft",
                        "sender": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                        "type": "0x2::devnet_nft::MintNFTEvent",
                        "fields": {
                            "creator": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                            "name": "Example NFT",
                            "object_id": "0xd5657cf6acaba958c5b01ec0516f4f0dac77c7d2"
                        },
                        "bcs": "1WV89qyrqVjFsB7AUW9PDax3x9L+1JBtcbilg//9jpVnYCe2u4HXzwtFeGFtcGxlIE5GVA=="
                    }
                }
            },
            {
                "timestamp": 1666698739180,
                "txDigest": "WF2V6FM6y/kpAgRqzsQmR/osy4pmTgVVbE6qvSJxWh4=",
                "id": {
                    "txSeq": 998,
                    "eventSeq": 0,
                },
                "event": {
                    "newObject": {
                        "packageId": "0x0000000000000000000000000000000000000002",
                        "transactionModule": "devnet_nft",
                        "sender": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                        "recipient": {
                            "AddressOwner": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf"
                        },
                        "objectId": "0xd5657cf6acaba958c5b01ec0516f4f0dac77c7d2"
                    }
                }
            }
        ],
        "nextCursor": null
    },
    "id": 1
}
```

#### 2. Tüm 0x2::devnet\_nft::MintNFTEvent etkinliklerini alın

#### İstek

```
curl --location --request POST '127.0.0.1:9000' \
--header 'Content-Type: application/json' \
--data-raw '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "sui_getEvents",
  "params": [
    {"MoveEvent":"0x2::devnet_nft::MintNFTEvent"},
    null,
    null,
    "Ascending"
  ]
}'
```

**Yanıt**

```
{
    "jsonrpc": "2.0",
    "result": {
        "data": [
            {
                "timestamp": 1666699837426,
                "txDigest": "cZXsToU6r0Uia6HIAwvr1eMlGsrg6b9+2oYZAskJ0wc=",
                "id": {
                    "txSeq": 1001,
                    "eventSeq": 1,
                },
                "event": {
                    "moveEvent": {
                        "packageId": "0x0000000000000000000000000000000000000002",
                        "transactionModule": "devnet_nft",
                        "sender": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                        "type": "0x2::devnet_nft::MintNFTEvent",
                        "fields": {
                            "creator": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                            "name": "Example NFT",
                            "object_id": "0x2ee80b4a2d203365dfbd68a90a8ad9a0dca19155"
                        },
                        "bcs": "LugLSi0gM2XfvWipCorZoNyhkVX+1JBtcbilg//9jpVnYCe2u4HXzwtFeGFtcGxlIE5GVA=="
                    }
                }
            },
            {
                "timestamp": 1666698739180,
                "txDigest": "WF2V6FM6y/kpAgRqzsQmR/osy4pmTgVVbE6qvSJxWh4=",
                "id": {
                    "txSeq": 998,
                    "eventSeq": 1,
                },
                "event": {
                    "moveEvent": {
                        "packageId": "0x0000000000000000000000000000000000000002",
                        "transactionModule": "devnet_nft",
                        "sender": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                        "type": "0x2::devnet_nft::MintNFTEvent",
                        "fields": {
                            "creator": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                            "name": "Example NFT",
                            "object_id": "0xd5657cf6acaba958c5b01ec0516f4f0dac77c7d2"
                        },
                        "bcs": "1WV89qyrqVjFsB7AUW9PDax3x9L+1JBtcbilg//9jpVnYCe2u4HXzwtFeGFtcGxlIE5GVA=="
                    }
                }
            }
        ],
        "nextCursor": null
    },
    "id": 1
}
```

#### 3. Tüm etkinlikleri alın ve azalan zaman sırasına göre sayfa başına 2 öğe döndürün <a href="#3-get-all-events-and-return-2-items-per-page-in-descending-time-order" id="3-get-all-events-and-return-2-items-per-page-in-descending-time-order"></a>

**İstek**

```
curl --location --request POST '127.0.0.1:9000' \
--header 'Content-Type: application/json' \
--data-raw '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "sui_getEvents",
  "params": [
    "All",
    null,
    2,
    "Ascending"
  ]
}'
```

**Yanıt**

```
{
    "jsonrpc": "2.0",
    "result": {
        "data": [
            {
                "timestamp": 1666698739180,
                "txDigest": "WF2V6FM6y/kpAgRqzsQmR/osy4pmTgVVbE6qvSJxWh4=",
                "id": {
                    "txSeq": 998,
                    "eventSeq": 0,
                },
                "event": {
                    "newObject": {
                        "packageId": "0x0000000000000000000000000000000000000002",
                        "transactionModule": "devnet_nft",
                        "sender": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                        "recipient": {
                            "AddressOwner": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf"
                        },
                        "objectId": "0xd5657cf6acaba958c5b01ec0516f4f0dac77c7d2"
                    }
                }
            },
            {
                "timestamp": 1666698739180,
                "txDigest": "WF2V6FM6y/kpAgRqzsQmR/osy4pmTgVVbE6qvSJxWh4=",
                "id": {
                    "txSeq": 998,
                    "eventSeq": 1,
                },
                "event": {
                    "moveEvent": {
                        "packageId": "0x0000000000000000000000000000000000000002",
                        "transactionModule": "devnet_nft",
                        "sender": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                        "type": "0x2::devnet_nft::MintNFTEvent",
                        "fields": {
                            "creator": "0xfed4906d71b8a583fffd8e95676027b6bb81d7cf",
                            "name": "Example NFT",
                            "object_id": "0xd5657cf6acaba958c5b01ec0516f4f0dac77c7d2"
                        },
                        "bcs": "1WV89qyrqVjFsB7AUW9PDax3x9L+1JBtcbilg//9jpVnYCe2u4HXzwtFeGFtcGxlIE5GVA=="
                    }
                }
            }
        ],
        "nextCursor": 3
    },
    "id": 1
}
```

### Sui etkinliklerine abone olun

Önceki bölümlerde açıklanan etkinliklere abone olduğunuzda, filtrelemek istediğiniz etkinliklerle eşleştirmek için etkinlik filtreleri uygulayabilirsiniz.

### Etkinlik filtreleri

EventFilter'ı, olay akışına aboneliğinize dahil olan olayları filtrelemek için kullanabilirsiniz. EventFilter bir özellik veya özelliklerin bir kombinasyonu üzerinde filtrelemeyi destekler.

Filtreleri destekleyen özelliklerin listesi:

| Filtre         | Açıklama                                                                                                    | Etkinlik Türü için Geçerlidir                                                                           | JSON-RPC Parametresi Örneği                                       |
| -------------- | ----------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| Package        | Move paket kimliği                                                                                          | <p>MoveEvent<br>Publish<br>TransferObject<br>DeleteObject<br>NewObject</p>                              | `{"Package":"0x2"}`                                               |
| Module         | Move modül ismi                                                                                             | <p>MoveEvent<br>TransferObject<br>DeleteObject<br>NewObject</p>                                         | `{"Module":"devnet_nft"}`                                         |
| MoveEventType  | Move kodunda tanımlanan Move etkinlik türü                                                                  | MoveEvent                                                                                               | `{"MoveEventType":"0x2::devnet_nft::MintNFTEvent"}`               |
| MoveEventField | Move etkinlik nesnesindeki veri alanlarını kullanarak filtreleme                                            | MoveEvent                                                                                               | `{"MoveEventField":{ "path":"/name", "value":"Example NFT"}}`     |
| SenderAddress  | İşlemin başlatıldığı adres                                                                                  | <p>MoveEvent<br>Publish<br>TransferObject<br>DeleteObject<br>NewObject</p>                              | `{"SenderAddress": "0x70613f4f17ae1363f7a7e7251daab5c5b06f68c1"}` |
| EventType      | [Etkinlikler](https://docs.sui.io/devnet/build/event\_api#type-of-events) bölümünde açıklanan etkinlik türü | <p>MoveEvent<br>Publish<br>TransferObject<br>DeleteObject<br>NewObject<br>EpochChange<br>Checkpoint</p> | `{"EventType":"Publish"}`                                         |
| ObjectId       | Nesne ID'si                                                                                                 | <p>TransferObject<br>DeleteObject<br>NewObject</p>                                                      | `{"ObjectId":"0xe3a6bc7bf1dba4d17a91724009c461bd69870719"}`       |

#### Filtreleri Birleştirme <a href="#combining-filters" id="combining-filters"></a>

Filtreleri birleştirmek için birkaç operatör sağlıyoruz:

| Operatör | Açıklama                                                                                 | JSON-RPC Parametresi Örneği                                                                         |
| -------- | ---------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| And      | İki filtreyi birleştirir; boolean Ve operatörüyle aynı şekilde davranır                  | `{"And":[{"Package":"0x2"}, {"Module":"devnet_nft"}]}`                                              |
| Or       | İki filtreyi birleştirir; boolean Veya operatörüyle aynı şekilde davranır                | `{"Or":[{"Package":"0x2"}, {"Package":"0x1"}]}`                                                     |
| All      | Bir filtre listesini birleştirir; tüm filtreler etkinlikle eşleşirse true döndürür       | `{"All":[{"EventType":"MoveEvent"}, {"Package":"0x2"}, {"Module":"devnet_nft"}]}`                   |
| Any      | Bir filtre listesini birleştirir; herhangi bir filtre etkinlikle eşleşirse true döndürür | `{"Any":[{"EventType":"MoveEvent"}, {"EventType":"TransferObject"}, {"EventType":"DeleteObject"}]}` |

#### Birleştirilmiş Filtre Kullanma Örneği

Aşağıdaki örnekte, [Sui Client CLI](https://docs.sui.io/devnet/build/cli-client#creating-example-nfts) `create-example-nft` komutundan `0x2::devnet_nft` paketi tarafından yayılan Move etkinliklerine (`MoveEvent`) nasıl abone olunacağı gösterilmektedir:

```
>> {"jsonrpc":"2.0", "id": 1, "method": "sui_subscribeEvent", "params": [{"All":[{"EventType":"MoveEvent"}, {"Package":"0x2"}, {"Module":"devnet_nft"}]}]}
<< {"jsonrpc":"2.0","result":3121662727959200,"id":1}
```

Bu akıştaki aboneliğinizi iptal etmek için şunu kullanın:

```
>> {"jsonrpc":"2.0", "id": 1, "method": "sui_unsubscribeEvent", "params": [3121662727959200]}
<< {"jsonrpc":"2.0","result":true,"id":1}
```

Son güncelleme 12/12/2022, 5:13:17 PM

* [Kaynak Kodu](https://github.com/MystenLabs/sui/blob/devnet/doc/src/build/event\_api.md)
