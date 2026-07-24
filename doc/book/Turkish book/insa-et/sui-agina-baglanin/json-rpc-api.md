# JSON-RPC API

Sui ağına uzaktan prosedür çağrıları (RPC) yapma kılavuzuna hoş geldiniz. Bu belge, Sui'ye bağlanma ve Sui ağıyla etkileşim kurmak için Sui JSON-RPC API'sinin nasıl kullanılacağı konusunda size yol gösterir. Doğrulama için dApp işlemlerinizi [Sui validatorlarına](https://docs.sui.io/devnet/learn/architecture/validators) göndermek için RPC katmanını kullanın.

Bu kılavuz, API aracılığıyla Sui ağ etkileşimleriyle ilgilenen geliştiriciler için yararlıdır ve JSON girdilerini Move Call argümanlarıyla hizalamak için [SuiJSON formatıyla](https://docs.sui.io/devnet/build/sui-json) birlikte kullanılmalıdır.

CLI aracılığıyla Sui ağ etkileşimleri hakkında benzer bir kılavuz için [Sui CLI client](https://docs.sui.io/devnet/build/cli-client) belgelerine bakın.

[Sui binary'lerini yüklemek](https://docs.sui.io/devnet/build/install) için talimatları izleyin.

### **Bir Sui ağına bağlanın**

Devnet üzerindeki bir Sui Tam Node'una bağlanabilirsiniz. Sui ağına RPC çağrıları yapmaya başlamak için [Sui Devnet'e Bağlanma](https://docs.sui.io/devnet/build/devnet) konusundaki yönergeleri izleyin.

Kendi Sui Tam Node'unuzu yapılandırmak için [Sui Tam Node'u yapılandırma](https://docs.sui.io/devnet/build/fullnode) bölümüne bakın.

### **Sui SDK'ları**

Aşağıdakilerden herhangi birini kullanarak işlemleri imzalayabilir ve Sui ağı ile etkileşime geçebilirsiniz:

* [Sui Rust SDK](https://docs.sui.io/devnet/build/rust-sdk) , Rust dili JSON-RPC sarmalayıcı ve kripto yardımcı programlarından oluşan bir koleksiyon.
* [Sui TypeScript SDK](https://github.com/MystenLabs/sui/tree/main/sdk/typescript) ve [referans dosyaları.](https://www.npmjs.com/package/@mysten/sui.js)
* Mevcut tüm yöntemler için [Sui API Referansı](https://docs.sui.io/sui-jsonrpc).

### Sui JSON-RPC Örnekleri <a href="#sui-json-rpc-examples" id="sui-json-rpc-examples"></a>

Aşağıdaki bölümlerde Sui JSON-RPC API'sinin cURL komutları ile nasıl kullanılacağı gösterilmektedir. Mevcut tüm yöntemlerin en son listesi için [Sui API Referansı](https://docs.sui.io/sui-jsonrpc)'na bakın.

#### Rpc keşfi <a href="#rpc-discover" id="rpc-discover"></a>

Sui RPC sunucusu OpenRPC'nin [hizmet keşif yöntemini](https://spec.open-rpc.org/#service-discovery-method) destekler. JSON-RPC APIs hizmetimizi açıklayan belgeleri sağlamak için bir `rpc.discover` yöntemi eklenmiştir.

```
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{ "jsonrpc":"2.0", "method":"rpc.discover","id":1}'
```

#### Nesne Aktarımı <a href="#transfer-object" id="transfer-object"></a>

Bu bölümdeki örnekler transfer işlemlerinin nasıl oluşturulacağını göstermektedir. Örnek komutları kullanmak için, çift parantezler (\{{ example\_ID \}}) arasındaki değerleri gerçek değerlerle değiştirin.

Komutun başarılı olması için `{{coin_object_id}}` ve `{{gas_object_id}}` nesne kimliklerinin `{{owner_address}}` için belirtilen adrese ait olması gerekir. Nesne kimliklerini döndürmek için `sui_getOwnedObjectsByAddress` komutunu kullanın.

**Bir Sui coini bir adresten diğerine aktarmak için imzasız bir işlem oluşturun**

```
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "sui_transferObject",
  "params":[
    "{{owner_address}}",
    "{{object_id}}",
    "{{gas_object_id}}",
    {{gas_budget}},
    "{{to_address}}"],
}' | json_pp
```

Yanıt aşağıdakine benzeyecektir.

```
{
  "id" : 1,
  "jsonrpc" : "2.0",
  "result" : {
    "tx_bytes" : "VHJhbnNhY3Rpb25EYXRhOjoAAFHe8jecgzoGWyGlZ1sJ2KBFN8aZF7NIkDsM+3X8mrVCa7adg9HnVqUBAAAAAAAAACDOlrjlT0A18D0DqJLTU28ChUfRFtgHprmuOGCHYdv8YVHe8jecgzoGWyGlZ1sJ2KBFN8aZdZnY6h3kyWFtB38Wyg6zjN7KzAcBAAAAAAAAACDxI+LSHrFUxU0G8bPMXhF+46hpchJ22IHlpPv4FgNvGOgDAAAAAAAA="
  }
}

```

**Sui keytool kullanarak bir işlemi imzalayın**

```
sui keytool sign --address <owner_address> --data <tx_bytes>
```

Keytool bir anahtar oluşturur ve ardından imza ve açık anahtar bilgilerini döndürür.

**İmza ve açık key içeren bir işlemi yürütme**

```
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "sui_executeTransaction",
  "params": [ 
    "{{tx_bytes}}",
    "{{sig_scheme}}",
    "{{signature}}",
    "{{pub_key}}",
    "{{request_type}}"
  ]
}' | json_pp
```

`sui_transferObject` ile yerel aktarım, genel aktarımlara izin veren tüm nesneleri destekler. Bazı nesneler yerel olarak aktarılamaz ve bir [Move call](https://docs.sui.io/devnet/build/json-rpc#sui\_movecall) gerektirir. Yerel aktarımlar hakkında daha fazla bilgi için [Transactions](https://docs.sui.io/devnet/learn/transactions#native-transaction) bölümüne bakın.

#### **Move fonksiyonlarını çağırma**

Bu bölümdeki örnek komutlar Move fonksiyonlarının nasıl çağrılacağını göstermektedir.

#### **Bir Move çağrı işlemini yürütme**

Belirli bir paketin modülünde belirtilen işlevi çağırarak bir Move çağrı işlemi gerçekleştirin (Sui'deki akıllı sözleşmeler [Move](https://docs.sui.io/devnet/build/move) dilinde yazılır):

```
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{ 
  "jsonrpc": "2.0",
  "method": "sui_moveCall",
  "params": [
    "{{owner_address}}",
    "0x2",
    "coin",
    "transfer",
    ["0x2::sui::sui"],
    ["{{object_id}}", "{{recipient_address}}"],
    "{{gas_object_id}}",
     2000
  ],
  "id": 1 
}' | json_pp
```

Argümanlar aktarılır ve tip fonksiyon imzasından çıkarılır. Gaz kullanımı `gas_budget` tarafından sınırlandırılır. `Transfer` işlevi [Sui CLI client](https://docs.sui.io/devnet/build/cli-client#calling-move-code) belgelerinde daha ayrıntılı olarak açıklanmıştır.

`Coin` modülündeki `transfer` fonksiyonu (`sui_transferObject`) ile aynı amaca hizmet eder. Yerel bir aktarım daha verimli olduğu için örnekleme amacıyla kullanılır.

Bir Move çağrısının hangi `args`'ı kabul ettiği hakkında daha fazla bilgi edinmek için [SuiJSON](https://docs.sui.io/devnet/build/sui-json)'a bakın.

#### Bir Move paketi yayınlama

```
curl --location --request POST $SUI_RPC_HOST \
--header 'Content-Type: application/json' \
--data-raw '{ 
  "jsonrpc":"2.0",
  "method":"sui_publish",
  "params":[
    "{{owner_address}}",
    ["{{vector_of_compiled_modules}}"],
    "{{gas_object_id}}",
     10000
   ],
  "id":1
}' | json_pp
```

Bu uç nokta, paketin geçerli olduğundan emin olmak için uygun doğrulama ve bağlama işlemlerini gerçekleştirir. Bazı modüllerin [başlatıcıları](https://docs.sui.io/devnet/build/move/debug-publish#module-initializers) varsa, bu başlatıcılar Move'da yürütülür (bu, bir Move paketinin yayınlanması sürecinde yeni Move nesnelerinin oluşturulabileceği anlamına gelir). Modül başlatıcılarını çalıştırma ihtiyacı nedeniyle gaz bütçesi gereklidir.

Bir Move modülünü yayınlamak için ayrıca `{{vector_of_compiled_modules}}` eklemeniz gerekir. Bu alanın değerini oluşturmak için `sui move` komutunu kullanın. `sui move` komutu bytecode'un base64 olarak yazdırılmasını destekler:

```
sui move --path <move-module-path> build --dump-bytecode-as-base64
```

Paketin kaynaklarının konumunun `PATH_TO_PACKAGE` ortam değişkeninde olduğunu varsayarsak, örnek bir komut aşağıdakine benzer:

```
sui move --path $PATH_TO_PACKAGE/my_move_package build --dump-bytecode-as-base64

["oRzrCwUAAAAJAQAIAggUAxw3BFMKBV1yB88BdAjDAigK6wIFDPACQgAAAQEBAgEDAAACAAEEDAEAAQEBDAEAAQMDAgAABQABAAAGAgEAAAcDBAAACAUBAAEFBwEBAAEKCQoBAgMLCwwAAgwNAQEIAQcODwEAAQgQAQEABAYFBgcICAYJBgMHCwEBCAALAgEIAAcIAwABBwgDAwcLAQEIAAMHCAMBCwIBCAADCwEBCAAFBwgDAQgAAgsCAQkABwsBAQkAAQsBAQgAAgkABwgDAQsBAQkAAQYIAwEFAgkABQMDBwsBAQkABwgDAQsCAQkAAgsBAQkABQdNQU5BR0VEBENvaW4IVHJhbnNmZXIJVHhDb250ZXh0C1RyZWFzdXJ5Q2FwBGJ1cm4EaW5pdARtaW50DHRyYW5zZmVyX2NhcAtkdW1teV9maWVsZA9jcmVhdGVfY3VycmVuY3kGc2VuZGVyCHRyYW5zZmVyAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgACAQkBAAEAAAEECwELADgAAgEAAAAICwkSAAoAOAEMAQsBCwAuEQY4AgICAQAAAQULAQsACwI4AwIDAQAAAQQLAAsBOAQCAA==", "oRzrCwUAAAALAQAOAg4kAzJZBIsBHAWnAasBB9IC6QEIuwQoBuMECgrtBB0MigWzAQ29BgYAAAABAQIBAwEEAQUBBgAAAgAABwgAAgIMAQABBAQCAAEBAgAGBgIAAxAEAAISDAEAAQAIAAEAAAkCAwAACgQFAAALBgcAAAwEBQAADQQFAAIVCgUBAAIICwMBAAIWDQ4BAAIXERIBAgYYAhMAAhkCDgEABRoVAwEIAhsWAwEAAgsXDgEAAg0YBQEABgkHCQgMCA8JCQsMCw8MFAYPBgwNDA0PDgkPCQMHCAELAgEIAAcIBQILAgEIAwsCAQgEAQcIBQABBggBAQMEBwgBCwIBCAMLAgEIBAcIBQELAgEIAAMLAgEIBAMLAgEIAwEIAAEGCwIBCQACCwIBCQAHCwcBCQABCAMDBwsCAQkAAwcIBQELAgEJAAEIBAELBwEIAAIJAAcIBQELBwEJAAEIBgEIAQEJAAIHCwIBCQALAgEJAAMDBwsHAQkABwgFAQYLBwEJAAZCQVNLRVQHTUFOQUdFRARDb2luAklEA1NVSQhUcmFuc2ZlcglUeENvbnRleHQHUmVzZXJ2ZQRidXJuBGluaXQObWFuYWdlZF9zdXBwbHkEbWludApzdWlfc3VwcGx5DHRvdGFsX3N1cHBseQtkdW1teV9maWVsZAJpZAtWZXJzaW9uZWRJRAx0cmVhc3VyeV9jYXALVHJlYXN1cnlDYXADc3VpB21hbmFnZWQFdmFsdWUId2l0aGRyYXcPY3JlYXRlX2N1cnJlbmN5Bm5ld19pZAR6ZXJvDHNoYXJlX29iamVjdARqb2luAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAgMIAAAAAAAAAAAAAgEOAQECBA8IBhELBwEIABMLAgEIAxQLAgEIBAABAAAIFg4BOAAMBAsBCgAPADgBCgAPAQoECgI4AgwFCwAPAgsECwI4AwwDCwULAwIBAAAAEA8JEgAKADgEDAEKABEKCwEKADgFCwA4BhIBOAcCAgEAAAMECwAQAjgIAgMBAAAFHA4BOAkMBAoEDgI4CCEDDgsAAQsDAQcAJwoADwELATgKCgAPAgsCOAsLBAsADwALAzgMAgQBAAADBAsAEAE4CQIFAQAAAwQLABAAOA0CAQEBAgEDAA=="]
Build Successful
```

Derlenen Move modülünün çıktı base64 gösterimini REST yayınlama uç noktasına kopyalayın.

Komut, yayınlanan Move kodunu temsil eden bir paket nesnesi oluşturur. Paket kimliğini, bu pakette tanımlanan işlevlere yönelik sonraki Move çağrıları için bir argüman olarak kullanabilirsiniz.
