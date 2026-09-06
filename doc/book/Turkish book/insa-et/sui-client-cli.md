# Sui Client CLI

Sui Client Komut Satırı Arayüzünü (CLI) nasıl kuracağınızı, yapılandıracağınızı ve kullanacağınızı öğrenin. Bir komut satırı arayüzü kullanarak Sui özelliklerini denemek için CLI'yi kullanabilirsiniz.

### Kurulum <a href="#set-up" id="set-up"></a>

SUI Client CLI, Sui'yi yüklediğinizde yüklenir. Ön koşullar ve yükleme talimatları için [Sui'yi Yükleme ](https://docs.sui.io/devnet/build/install)konusuna bakın.

### Sui Client'ını kullanma <a href="#using-the-sui-client" id="using-the-sui-client"></a>

Sui Client CLI aşağıdaki komutları destekler:

| Komut                    | Açıklama                                                                                                                                                                                                                                               |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `active-address`         | Hiçbiri belirtilmediğinde komutlar için kullanılan varsayılan adres                                                                                                                                                                                    |
| `active-env`             | Hiçbiri belirtilmediğinde komutlar için kullanılan varsayılan ortam                                                                                                                                                                                    |
| `addresses`              | Client tarafından yönetilen Adresleri edinin                                                                                                                                                                                                           |
| `call`                   | Move işlevini çağırın                                                                                                                                                                                                                                  |
| `create-example-nft`     | Örnek bir NFT oluşturun                                                                                                                                                                                                                                |
| `envs`                   | Tüm Sui ortamlarını listeleyin                                                                                                                                                                                                                         |
| `execute-signed-tx`      | İmzalanmış Bir İşlemi Yürüt. Bu, kullanıcı başka bir yerde imzalamayı tercih ettiğinde ve bu komutu çalıştırmak için kullandığında kullanışlıdır                                                                                                       |
| `gas`                    | Adresin sahip olduğu tüm gas nesnelerini elde edin                                                                                                                                                                                                     |
| `help`                   | Bu mesajı veya verilen alt komut(lar)ın yardımını yazdırır                                                                                                                                                                                             |
| `merge-coin`             | İki coin nesnesini tek bir coin olarak birleştirme                                                                                                                                                                                                     |
| `new-address`            | Ed25519 için varsayılan m/44'/784'/0'/0'/0' veya secp256k1 için varsayılan m/54'/784'/0'/0/0 olan isteğe bağlı türetme yolu ile {ed25519 veya secp256k1} anahtar çifti şeması flag'i ile yeni adres ve anahtar çifti oluşturun                         |
| `new-env`                | Yeni Sui ortamı ekleyin                                                                                                                                                                                                                                |
| `object`                 | Nesne bilgilerini al                                                                                                                                                                                                                                   |
| `objects`                | Adresin sahip olduğu tüm nesneleri elde edin                                                                                                                                                                                                           |
| `pay`                    | Belirtilen miktarları takip eden alıcılara giriş coinleri ile SUI ödeyin. Alıcıların uzunluğu tutarların uzunluğu ile aynı olmalıdır                                                                                                                   |
| `pay_all_sui`            | Gas maliyetini düştükten sonra kalan tüm SUI Coin'leri giriş Coin'leri ile alıcıya ödeyin. Giriş Coin'leri gaz ödemesi için kullanılan Coin'i de içerir, bu nedenle ekstra gas Coin'e gerek yoktur                                                     |
| `pay_sui`                | Belirtilen miktarları takip eden alıcılara giriş paraları ile SUI coinleri ödeyin. Alıcıların uzunluğu tutarların uzunluğu ile aynı olmalıdır. Giriş Coin'leri gas ödemesi için kullanılan Coin'i de içerir, bu nedenle ekstra gas Coin'e gerek yoktur |
| `publish`                | Move modüllerini yayınlayın                                                                                                                                                                                                                            |
| `serialize-transfer-sui` | İmzalanabilen bir aktarımı serileştirin. Bu, kullanıcı verileri imzalamak için başka bir yere götürmeyi tercih ettiğinde kullanışlıdır                                                                                                                 |
| `split-coin`             | Coin nesnesini birden çok coine bölme                                                                                                                                                                                                                  |
| `switch`                 | Etkin adresi ve ağı değiştir (örneğin, devnet, yerel rpc sunucusu)                                                                                                                                                                                     |
| `sync`                   | Client durumunu yetkililerle senkronize edin                                                                                                                                                                                                           |
| `transfer`               | Nesneyi aktarın                                                                                                                                                                                                                                        |
| `transfer-sui`           | SUI aktarır ve aynı SUI coin nesnesi ile gas öder. Miktar belirtilmişse, yalnızca miktarı aktarır. Belirtilmemişse, nesneyi aktarır.                                                                                                                   |

**Note:** `clear`, `echo`, `env` ve `exit`komutları yalnızca etkileşimli shell'de bulunur.

Desteklenen komutların listesini görmek için `sui client -h` komutunu kullanın.

Her komut hakkında daha fazla bilgi görmek için `sui help command` kullanın.

Client'ı iki modda başlatabilirsiniz: etkileşimli shell veya komut satırı arayüzü Sui client'ını yapılandırın.

#### İnteraktif shell <a href="#interactive-shell" id="interactive-shell"></a>

Etkileşimli kabuğu başlatmak için:

Konsol komutu `~/.sui/sui_config` dizininde `client.yaml` client yapılandırma dosyasını arar. Ancak bu dosyanın depolandığı dizine bir yol sağlayarak bu ayarı geçersiz kılabilirsiniz:

```
sui console --config /workspace/config-files
```

Sui interaktif client konsolu aşağıdaki shell işlevlerini destekler:

* _Komut geçmişi_ - komut geçmişini yazdırmak için `history` komutunu kullanın. Geçmiş listesinde bir önceki veya bir sonrakini görüntülemek için Yukarı, Aşağı veya Ctrl-P, Ctrl-N tuşlarını da kullanabilirsiniz. Komut geçmişinde arama yapmak için Ctrl-R'yi kullanın.
* _Sekme tamamlama_ - Sekme ve Ctrl-I tuşlarını kullanan tüm komutlar için desteklenir.
* _Ortam değişkeni ikamesi_ - konsol, `$` ile ön eklenmiş girdiyi ortam değişkenleriyle değiştirir. Değişkenlerin tüm listesini yazdırmak için `env` komutunu kullanın ve herhangi bir komut çağırmadan ikameyi önizlemek için `echo`'yu kullanın.

#### Komut satırı modu <a href="#command-line-mode" id="command-line-mode"></a>

Client'ı etkileşimli shell olmadan kullanabilirsiniz. Bu, client'ın çıktısını başka bir uygulamaya aktarmak veya komut dosyaları kullanarak client komutlarını çağırmak istiyorsanız kullanışlıdır.

```
USAGE:
    sui client [SUBCOMMAND]
```

Örneğin, aşağıdaki komut platformda bulunan hesap adreslerinin listesini döndürür:

#### Aktif adres <a href="#active-address" id="active-address"></a>

Komutları yürütmek için kullanılacak etkin bir adres veya varsayılan adres belirleyebilirsiniz.

Sui, komutlar için kullanılacak varsayılan bir adres belirler. Adres gerektiren komutlar için etkin adresi kullanır. Geçerli etkin adresi görüntülemek için `active-address` komutunu kullanın.

```
sui client active-address
```

Talebe verilen yanıt aşağıdakine benzer:

```
0x562f07cf6369e8d22dbf226a5bfedc6300014837
```

Varsayılan adresi değiştirmek için `switch` komutunu kullanın:

```
sui client switch --address 0x913cf36f370613ed131868ac6f9da2420166062e
```

Yanıt aşağıdakine benzer:

```
Active address switched to 0x913cf36f370613ed131868ac6f9da2420166062e
```

`objects` komutunu bir adres belirterek ya da belirtmeden çağırabilirsiniz. Bir adres belirtmezseniz Sui etkin adresi kullanır.

```
sui client objects
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55 |     0      | j8qLxVk/Bm9iMdhPf9b7HcIMQIAM+qCd8LfPAwKYrFo= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
```

```
sui client objects 0x913cf36f370613ed131868ac6f9da2420166062e
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55 |     0      | j8qLxVk/Bm9iMdhPf9b7HcIMQIAM+qCd8LfPAwKYrFo= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
```

Adresi atlayan sonraki tüm komutlar yeni etkin `adres`'i kullanır: 0x913cf36f370613ed131868ac6f9da2420166062e

Etkin adrese ait olmayan bir gas nesnesi kullanan bir komut çağırırsanız, Sui geçici olarak işlem için gaz nesnesine sahip olan adresi kullanır.

#### Gas nesneleri ile yapılan işlemler için ödeme <a href="#paying-for-transactions-with-gas-objects" id="paying-for-transactions-with-gas-objects"></a>

Tüm Sui işlemleri ödeme için bir gas nesnesinin yanı sıra bir bütçe de gerektirir. Bununla birlikte, gas nesnesini belirtmek zahmetli olabilir; bu nedenle CLI'da gas nesnesinin atlanmasına ve müşterinin belirtilen bütçeyi karşılayan bir nesneyi seçmesine izin verilir. Bu gas seçim mantığı şu anda ilkeldir çünkü gazı gerektiği gibi birleştirmez/bölmez ancak şu anda bütçeyi karşılayan bulduğu ilk nesneyi seçer. Gas'i kendileri yönetmek isterlerse her zaman kendi gazlarını belirleyebileceklerini unutmayın.

⚠️Bir gas nesnesi hem işlemin bir parçası olup hem de işlem için ödeme yapmak için kullanılamaz. Örneğin, X gas nesnesi ile işlem için ödeme yaparken X gas nesnesini aktarmaya çalışamazsınız. Gas seçim mantığı bunu kontrol eder ve bu tür durumları reddeder.

Bir hesapta ne kadar gas olduğunu görmek için `gas` komutunu kullanın. Aksi belirtilmedikçe bu komutun `active-address`'i kullandığını unutmayın.

```
sui client gas
```

Aktif adres yerine o adrese ait gas miktarını görmek için bir adres belirtebilirsiniz.

```
sui client gas 0x562f07cf6369e8d22dbf226a5bfedc6300014837
```

### Yeni hesap adresleri oluşturun <a href="#create-new-account-addresses" id="create-new-account-addresses"></a>

Sui Client CLI varsayılan olarak 1 adres içerir. Daha fazla adres eklemek için, `new-address` komutu ile client için yeni adresler oluşturun veya mevcut hesapları client.yaml dosyasına ekleyin.

#### Yeni bir hesap adresi oluşturun <a href="#create-a-new-account-address" id="create-a-new-account-address"></a>

```
sui client new-address ed25519
```

`Ed25519` veya `secp256k1` anahtar şemasını belirtmeniz gerekir.

#### Mevcut hesapları client.yaml dosyasına ekleyin <a href="#add-existing-accounts-to-clientyaml" id="add-existing-accounts-to-clientyaml"></a>

Client'ınıza mevcut hesap adreslerini eklemek için, örneğin önceki bir kurulumdan, client.yaml dosyasını düzenleyin ve hesaplar bölümünü ekleyin. Ayrıca anahtar deposu dosyasına anahtar çifti eklemeniz gerekir.

Değişiklikleri client.yaml dosyasına kaydettikten sonra Sui konsolunu yeniden başlatın.

### Bir adresin sahip olduğu nesneleri görüntüleme <a href="#view-objects-an-address-owns" id="view-objects-an-address-owns"></a>

Bir adresin sahip olduğu nesneleri görüntülemek için `objects` komutunu kullanın.

```
sui client objects
```

Etkin adresten farklı bir adrese ait nesneleri görüntülemek için, nesnelerin görüleceği adresi belirtin.

```
sui client objects 0x66af3898e7558b79e115ab61184a958497d1905a
```

Bir nesne hakkında daha fazla bilgi görüntülemek için `object` komutunu kullanın.

```
    sui client object <ID>
```

Sonuç, nesne, sahibi, sürümü, kimliği, nesnenin değişmez olup olmadığı ve nesnenin türü hakkında bazı temel bilgileri gösterir.

Nesnenin JSON gösterimini görüntülemek için komuta `--json` ekleyin.

```
    sui client object <ID> --json
```

### Nesneleri aktarın <a href="#transfer-objects" id="transfer-objects"></a>

Sahip olduğunuz değiştirilebilir nesneleri aşağıdaki komutu kullanarak başka bir adrese aktarabilirsiniz

```
    sui client transfer [OPTIONS] --to <TO> --object-id <OBJECT_ID> --gas-budget <GAS_BUDGET>

OPTIONS:
        --coin-object-id <OBJECT_ID>
            Object to transfer, in 20 bytes Hex string

        --gas <GAS>
            ID of the gas object for gas payment, in 20 bytes Hex string If not provided, a gas
            object with at least gas_budget value will be selected

        --gas-budget <GAS_BUDGET>
            Gas budget for this transfer

    -h, --help
            Print help information

        --json
            Return command outputs in json format

        --to <TO>
            Recipient address
```

Bir nesneyi bir alıcıya aktarmak için alıcının adresine, aktarılacak nesnenin nesne kimliğine ve isteğe bağlı olarak işlem ücreti ödemesi için coin nesnesinin kimliğine ihtiyacınız vardır. Belirtilmezse, bütçeyi karşılayan bir coin seçilir. Gas budget, ne kadar gaz harcanacağına dair bir üst sınır belirler. Gas ölçüm mekanizmalarımızı hala tamamlıyoruz. Şimdilik, sadece yeterince büyük bir şey ayarlayın.

```
sui client transfer --to 0xf456ebef195e4a231488df56b762ac90695be2dd --object-id 0x66eaa38c8ea99673a92a076a00101ab9b3a06b55 --gas-budget 100
```

### Örnek bir NFT oluşturun <a href="#create-an-example-nft" id="create-an-example-nft"></a>

`create-example-nft` komutunu kullanarak bir adrese örnek bir NFT ekleyebilirsiniz. Komut, etkin adrese bir NFT ekler.

```
sui client create-example-nft
```

Komut, `devnet_nft` modülündeki `mint` işlevini çağırarak üç özniteliğe sahip bir Sui nesnesi çıkarır: [varsayılan değerlerle](https://github.com/MystenLabs/sui/blob/27dff728a4c9cb65cd5d92a574105df20cb51887/sui/src/wallet\_commands.rs#L39) ad, açıklama ve görüntü URL'si ve nesneyi adresinize aktarır. Aşağıdaki talimatları kullanarak özel değerler de sağlayabilirsiniz:

`create-example-nft` komutu kullanımı:

```
    sui client create-example-nft [OPTIONS]

OPTIONS:
        --description <DESCRIPTION>    Description of the NFT
        --gas <GAS>                    ID of the gas object for gas payment, in 20 bytes Hex string
                                       If not provided, a gas object with at least gas_budget value
                                       will be selected
        --gas-budget <GAS_BUDGET>      Gas budget for this transfer
    -h, --help                         Print help information
        --json                         Return command outputs in json format
        --name <NAME>                  Name of the NFT
        --url <URL>                    Display url(e.g., an image url) of the NFT

```

### Coin nesnelerini birleştirme ve bölme <a href="#merge-and-split-coin-objects" id="merge-and-split-coin-objects"></a>

Bir hesaptaki ayrı coin nesnelerinin sayısını azaltmak için coinleri birleştirebilir veya transferler veya gas ödemeleri için kullanmak üzere daha küçük coin nesneleri oluşturmak için coinleri bölebilirsiniz.

Coinleri birleştirmek veya bölmek için sırasıyla `merge-coin` komutunu ve `split-coin` komutunu kullanabiliriz.

#### Coinleri birleştirme <a href="#merge-coins" id="merge-coins"></a>

```
    sui client merge-coin [OPTIONS] --primary-coin <PRIMARY_COIN> --coin-to-merge <COIN_TO_MERGE> --gas-budget <GAS_BUDGET>

OPTIONS:
        --coin-to-merge <COIN_TO_MERGE>
            Coin to be merged, in 20 bytes Hex string

        --gas <GAS>
            ID of the gas object for gas payment, in 20 bytes Hex string If not provided, a gas
            object with at least gas_budget value will be selected

        --gas-budget <GAS_BUDGET>
            Gas budget for this call

    -h, --help
            Print help information

        --json
            Return command outputs in json format

        --primary-coin <PRIMARY_COIN>
            Coin to merge into, in 20 bytes Hex string
```

Coin'leri birleştirmek için en az üç coin nesnesine, birleştirmek için iki coine ve gas ödemesi için bir ödemeye ihtiyacınız vardır. Bir coini birleştirdiğinizde, birleştirme işlemi için izin verilen maksimum gas bütçesini belirtirsiniz.

Belirtilen adresin sahip olduğu nesneleri görüntülemek için aşağıdaki komutu kullanın.

```
sui client objects 0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75
```

Önceki komuttan dönen ID'leri `merge-coin` komutunda kullanın.

```
sui client merge-coin --primary-coin 0x1e90389f5d70d7fa6ce973155460e1c04deae194 --coin-to-merge 0x351f08f03709cebea85dcd20e24b00fbc1851c92 --gas-budget 1000
```

#### Coinleri bölme <a href="#split-coins" id="split-coins"></a>

```
    sui client split-coin [OPTIONS] --coin-id <COIN_ID> --gas-budget <GAS_BUDGET> (--amounts <AMOUNTS>... | --count <COUNT>)

OPTIONS:
        --amounts <AMOUNTS>...       Specific amounts to split out from the coin
        --coin-id <COIN_ID>          Coin to Split, in 20 bytes Hex string
        --count <COUNT>              Count of equal-size coins to split into
        --gas <GAS>                  ID of the gas object for gas payment, in 20 bytes Hex string If
                                     not provided, a gas object with at least gas_budget value will
                                     be selected
        --gas-budget <GAS_BUDGET>    Gas budget for this call
    -h, --help                       Print help information
        --json                       Return command outputs in json format
```

Bir coin'i bölmek için en az 2 coin nesnesine ihtiyacınız vardır, biri bölmek için diğeri de gas ücretlerini ödemek için.

Adresin sahip olduğu nesneleri görüntülemek için aşağıdaki komutu kullanın.

```
sui client objects 0x08da15bee6a3f5b01edbbd402654a75421d81397
```

Ardından `split-coin` komutunda döndürülen kimlikleri kullanın.

Aşağıdaki örnek, bir coin'i 1000, 5000 ve 3000 olmak üzere farklı miktarlarda üç coin'e böler. `--amounts` bağımsız değişkeni bir değerler listesi kabul eder.

```
sui client split-coin --coin-id 0x4a2853304fd2c243dae7d1ba58260bb7c40724e1 --amounts 1000 5000 3000 --gas-budget 1000
```

Yeni coin nesnelerini görüntülemek için `objects` komutunu kullanın.

```
sui client objects 0x08da15bee6a3f5b01edbbd402654a75421d81397
```

Aşağıdaki örnek bir coin'i üç eşit parçaya böler. Bir coin'i eşit olarak bölmek için komuta `--amount` argümanını eklemeyin.

```
sui client split-coin --coin-id 0x4a2853304fd2c243dae7d1ba58260bb7c40724e1 --count 3 --gas-budget 1000
```

### Move kodunu çağırma (call) <a href="#calling-move-code" id="calling-move-code"></a>

Sui platformunun genesis durumu, Sui CLI'dan çağrılmaya hemen hazır olan Move kodunu içerir. Move kaynak koduna ilk bakış ve bu eğitimde çağıracağımız aşağıdaki işlevin açıklaması için lütfen [Move geliştirici belgelerimize](https://docs.sui.io/devnet/build/move#first-look-at-move-source-code) bakın:

```
public entry fun transfer(c: coin::Coin<SUI>, recipient: address) {
    transfer::transfer(c, Address::new(recipient))
}
```

Coin'leri transfer etmek için bir Move çağrısı kullanmaya gerçekten gerek olmadığını, bunun yerleşik bir Sui [client komutuyla](https://docs.sui.io/devnet/build/cli-client#transferring-coins) gerçekleştirilebileceğini lütfen unutmayın - basitliği nedeniyle bu örneği seçtik.

`0x48ff0a932b12976caec91d521265b009ad5b2225` adresine ait nesneleri inceleyelim:

```
sui client objects 0x48ff0a932b12976caec91d521265b009ad5b2225

                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x471c8e241d0473c34753461529b70f9c4ed3151b |     0      | MCQIALghS9kQUWMclChmsd6jCuLiUxNjEn9VRV+AhSA= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0x53b50e3020a01e1fd6acf832a871feee240183f0 |     0      | VIbuA4fcsitOUmJLQ+FugZWIn7bg6LnVO8eTIAUDzkg= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0x5c846224b8704683a1c576aec7c8d9c3413d87c1 |     0      | KO0Fr9uCPnT3KxOEishyzas33le4J9fAGg7iEOOzo7A= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0x6fe4cf8d2c21f23f2aacf60f30c98ff9e2c78226 |     0      | p2evKbTirwEoF1PxGIu5USAsSdkxzh1sUD/OxBfpdNE= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0xa28dd252ab5b984a8c1da699bbe10e7f09947a12 |     0      | 6VT+8479aijA8tYmab7YatVgjXm1TWy5jItooC416YQ= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
Showing 5 results.
```

Artık hangi nesnelerin bu adrese ait olduğunu bildiğimize göre, bunlardan birini başka bir adrese, örneğin [Yeni bir hesap oluşturma](https://docs.sui.io/devnet/build/cli-client#generating-a-new-account) bölümünde oluşturduğumuz yeni bir adrese (`0xc72cf3adcc4d11c03079cef2c8992aea5268677a`) aktarabiliriz. Herhangi bir nesneyi deneyebiliriz, ancak bu alıştırmanın iyiliği için listedeki sonuncuyu seçelim.

Aşağıdaki Sui client komutunu kullanarak sui modülünden `transfer` fonksiyonunu çağırarak transferi gerçekleştireceğiz:

```
sui client call --function transfer --module sui --package 0x2 --args 0x471c8e241d0473c34753461529b70f9c4ed3151b 0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75 --gas-budget 1000
```

Bu oldukça karmaşık bir komuttur, bu nedenle tüm parametrelerini tek tek açıklayalım:

* `--function` - çağrılacak fonksiyonun adı
* `--module` - fonksiyonu içeren modülün adı
* `--package` -Fonksiyonu içeren modülün bulunduğu paket nesnesinin kimliği. (GAS modülünü içeren genesis Sui paketinin kimliğinin manifesto dosyasında tanımlandığını ve `0x2`'ye eşit olduğunu unutmayın).
* `--args` - [SuiJSON](https://docs.sui.io/devnet/build/sui-json) değerleri olarak biçimlendirilmiş fonksiyon argümanlarının bir listesi (dolayısıyla adres ve nesne kimliğinde önceki `0x`):
  * `transfer` fonksiyonunun `c` parametresini temsil eden gaz nesnesinin ID'si
  * yeni gas nesnesi sahibinin adresi
* `--gas` - bu fonksiyon çağrısı için ödeme yapmak üzere kullanılan gas'i içeren isteğe bağlı bir nesne
* `--gas-budget` - gaz ödemesindeki tüm gas'in yanlışlıkla boşaltılmasını önlemek için `transfer` çağrısının tamamlanması için ne kadar gas ödemeye istekli olduğumuzu ifade eden ondalık bir değer)

`TxContext`'i temsil eden `transfer` işlevinin üçüncü argümanının açıkça belirtilmesi gerekmediğine dikkat edin - Sui'den çağrılabilen tüm işlevler için gerekli bir argümandır ve işlev çağrısı noktasında platform tarafından otomatik olarak enjekte edilir.

> **Önemli:** Köşeli parantezleri (\[ ]) özel karakterler olarak yorumlayan bir shell kullanıyorsanız (zsh shell gibi), parantezleri tek tırnak içine almalısınız. Örneğin, `[7,42]` yerine `'[7,42]'` kullanmalısınız.
>
> Ayrıca, nesne kimliklerinden oluşan bir vektör belirttiğinizde, her kimliği çift tırnak içine almanız gerekir. For example, `'["0x471c8e241d0473c34753461529b70f9c4ed3151b","0x53b50e3020a01e1fd6acf832a871feee240183f0"]'`

Nesneye daha derin bir bakış elde etmek için

> Nesnenin ham JSON gösterimini görmek için `sui client` komutundaki `--json` flag'i.

Call komutunun çıktısı biraz ayrıntılıdır, ancak sonunda yazdırılması gereken önemli bilgiler, işlev çağrısının bir sonucu olarak nesnelerin değiştiğini gösterir:

```
----- Certificate ----
Transaction Hash: KT7sEHzxavRFkLijfKGDqj6kM5bVl1QA1IawJPV2+Go=
Transaction Signature: GIUaa8yAPgy/eSVypVz+fmbjC2mL5kHuYNodUyNcIUMvlUN5XxyPYdL8C25vvH6rYt/ZUDY2ntZU1NHUp4yPCg==@iocJzkLCMJMh1VGZ6sUsw0okqoDP71ed9a4Vf2vWlx4=
Signed Authorities : [k#5067c1e30cc9d8b9ed9fe589beffbcdd14a2829b9fed5bf602608f411dbc4d56, k#e5b3bc0d482603d8b54a25246b9053e958c872530d4014676d5c30d885f116ac, k#3adde8bfae7d338b65e7d13d4ead6b523e5271ca17b2d5eb321412257ee914a4]
Transaction Kind : Call
Package ID : 0x2
Module : sui
Function : transfer
Arguments : ["0x471c8e241d0473c34753461529b70f9c4ed3151b", "0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75"]
Type Arguments : []
----- Transaction Effects ----
Status : Success
Mutated Objects:
  - ID: 0x471c8e241d0473c34753461529b70f9c4ed3151b , Owner: Account Address ( 0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75 )
  - ID: 0x53b50e3020a01e1fd6acf832a871feee240183f0 , Owner: Account Address ( 0x48ff0a932b12976caec91d521265b009ad5b2225 )
```

Bu çıktı, gas nesnesinin işlev çağrısı için gas ödemesi toplamak üzere güncellendiğini ve aktarılan nesnenin sahibi değiştirildiği için güncellendiğini gösterir. İkincisini (ve dolayısıyla `transfer` fonksiyonunun başarılı bir şekilde yürütüldüğünü) şu anda gönderenin sahibi olduğu nesneleri sorgulayarak teyit edebiliriz:

```
sui client objects 0x48ff0a932b12976caec91d521265b009ad5b2225
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x53b50e3020a01e1fd6acf832a871feee240183f0 |     1      | st6KVE+nTPsQgtEtxSbgJZCzSSuSB2ZsJAMbXFNLw/k= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0x5c846224b8704683a1c576aec7c8d9c3413d87c1 |     0      | KO0Fr9uCPnT3KxOEishyzas33le4J9fAGg7iEOOzo7A= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0x6fe4cf8d2c21f23f2aacf60f30c98ff9e2c78226 |     0      | p2evKbTirwEoF1PxGIu5USAsSdkxzh1sUD/OxBfpdNE= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0xa28dd252ab5b984a8c1da699bbe10e7f09947a12 |     0      | 6VT+8479aijA8tYmab7YatVgjXm1TWy5jItooC416YQ= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
Showing 4 results.
```

Artık bu adresin aktarılan nesneye sahip olmadığını görebiliyoruz. Ve bu nesneyi incelersek, orijinal sahibinden farklı yeni bir sahibi olduğunu görebiliriz:

```
sui client object 0x471c8e241d0473c34753461529b70f9c4ed3151b
```

Sonuç olarak:

```
----- Move Object (0x471c8e241d0473c34753461529b70f9c4ed3151b[1]) -----
Owner: Account Address ( 0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75 )
Version: 1
Storage Rebate: 15
Previous Transaction: KT7sEHzxavRFkLijfKGDqj6kM5bVl1QA1IawJPV2+Go=
----- Data -----
type: 0x2::coin::Coin<0x2::sui::SUI>
balance: 100000
id: 0x471c8e241d0473c34753461529b70f9c4ed3151b[1]
```

### Paketleri yayınlama <a href="#publish-packages" id="publish-packages"></a>

Geliştirdiğiniz kodun Sui'de kullanılabilir olması için paketleri Sui [dağıtılmış ledger'ına](https://docs.sui.io/devnet/learn/how-sui-works#architecture) yayınlamanız gerekir. Sui client'i ile paketleri yayınlamak için `publish` komutunu kullanın.

Daha sonra Sui client `publish` komutunu kullanarak yayınlayabileceğiniz [basit bir Move kod paketinin](https://docs.sui.io/devnet/build/move/write-package) nasıl yazılacağı hakkında açıklama için [Move geliştirici belgelerine](https://docs.sui.io/devnet/build/move) bakın.

**Önemli:** Yeni modülü yayınlamadan önce `debug` (hata ayıklama) modülündeki işlevlere yapılan tüm çağrıları test edilmeyen koddan kaldırmanız gerekir (test kodu `#[test]` ek açıklamasıyla işaretlenir).

Yayınlama için [önceki ](https://docs.sui.io/devnet/build/cli-client#calling-move-code)bölümde Move kodunu çağırmak için kullandığımız adresi kullanın `(0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75)`ve şimdi dört nesne kaldı:

```
sui client objects 0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75
```

Çıkış:

```
                 Object ID                  |  Version   |                    Digest                    |   Owner Type    |               Object Type
---------------------------------------------------------------------------------------------------------------------------------------------------------------------
 0x53b50e3020a01e1fd6acf832a871feee240183f0 |     1      | st6KVE+nTPsQgtEtxSbgJZCzSSuSB2ZsJAMbXFNLw/k= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0x5c846224b8704683a1c576aec7c8d9c3413d87c1 |     0      | KO0Fr9uCPnT3KxOEishyzas33le4J9fAGg7iEOOzo7A= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0x6fe4cf8d2c21f23f2aacf60f30c98ff9e2c78226 |     0      | p2evKbTirwEoF1PxGIu5USAsSdkxzh1sUD/OxBfpdNE= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
 0xa28dd252ab5b984a8c1da699bbe10e7f09947a12 |     0      | 6VT+8479aijA8tYmab7YatVgjXm1TWy5jItooC416YQ= |  AddressOwner   |      0x2::coin::Coin<0x2::sui::SUI>
Showing 4 results.
```

`0x3cbf06e9997b3864e3baad6bc0f0ef8ec423cd75` adresi için bir paket yayınlama komutunun tamamı aşağıdakine benzer (paket kaynaklarının konumunun `PATH_TO_PACKAGE` ortam değişkeninde olduğu varsayılarak):

```
sui client publish $PATH_TO_PACKAGE/my_move_package --gas 0xc8add7b4073900ffb0a8b4fe7d70a7db454c2e19 --gas-budget 30000 --verify-dependencies
```

publish komutu, paketinizin yolunu isteğe bağlı bir konumsal parametre olarak kabul eder (önceki çağrıda `$PATH_TO_PACKAGE/my_move_package`). Yolu belirtmezseniz, komut varsayılan yol değeri olarak geçerli çalışma dizinini kullanır. Çağrı ayrıca aşağıdaki verileri de sağlar:

* `--gas` - Gas ödemesi için kullanılan madeni para nesnesi.
* `--gas-budget` - Modül başlatıcılarını çalıştırmak için gas bütçesi.
* `--verify-dependencies` - CLI'nin tüm bağımlılıkların zincir üzerindeki karşılıklarıyla eşleşip eşleşmediğini kontrol etmesi için isteğe bağlı flag.

`--verify-dependencies` flag'i (Bağımlılıkları doğrula) mevcut olduğunda, CLI, ilgili yayınlanan adreslerinde bulunan bağımlılıkların bayt kodunun, bu bağımlılığı kaynak koddan derlerken elde ettiğiniz bayt koduyla eşleştiğini doğrular. Bir bağımlılığın bayt kodu eşleşmezse, paketiniz yayınlanmaz ve uyumsuzluğun hangi paket ve modülde bulunduğunu belirten bir hata mesajı alırsınız:

```
Local dependency did not match its on-chain version at <address>::<package>::<module>
```

`--verify-dependencies` flag'i başka nedenlerle de yayınlamayı başarısız kılabilir:

* Bağımlılığın yerel sürümünde veya zincir üzerinde eksik modüller var.
* Bağımlılığın işaret ettiği adreste hiçbir şey yok (silinmiş ya da hiç var olmamış).
* Bağımlılık için verilen adres bir paket yerine bir nesneye işaret eder.
* CLI, paketi almak için node'a bağlanamıyor.

Başarılı olursa, yanıtınız aşağıdakine benzer:

```
----- Certificate ----
Transaction Hash: evmJUz0+a2oFMbsTza2U+vC9q2KHeDVVV9XUma8OXv8=
Transaction Signature: 7Lqy/KQW86Tq81cUxLMW07AQw1S+D4QLFC9/jMNKrau81eABHpxG2lgaVaAh0c+d5ldYhp75SmpY0pxq0FSLBA==@BE/TaOYjyEtJUqF0Db4FEcVT4umrPmp760gFLQIGA1E=
Signed Authorities : [k#5067c1e30cc9d8b9ed9fe589beffbcdd14a2829b9fed5bf602608f411dbc4d56, k#f2e5749a5fc33d45c6f546eb9e53fabf4f17681ba6f697080de9514f4e0d6a75, k#e5b3bc0d482603d8b54a25246b9053e958c872530d4014676d5c30d885f116ac]
Transaction Kind : Publish
----- Publish Results ----
The newly published package object ID: 0xdbcee02bd4eb326122ced0a8540f15a057d82850

List of objects created by running module initializers:
----- Move Object (0x4ac2df49c3698baaef11ae23b3d8417d7e5ed65f[1]) -----
Owner: Account Address ( 0xb02b5e57fe3572f94ad5ac2a17392bfb3261f7a0 )
Version: 1
Storage Rebate: 12
Previous Transaction: evmJUz0+a2oFMbsTza2U+vC9q2KHeDVVV9XUma8OXv8=
----- Data -----
type: 0xdbcee02bd4eb326122ced0a8540f15a057d82850::m1::Forge
id: 0x4ac2df49c3698baaef11ae23b3d8417d7e5ed65f[1]
swords_created: 0

Updated Gas : Coin { id: 0xc8add7b4073900ffb0a8b4fe7d70a7db454c2e19, value: 96929 }
```

Bu komutu çalıştırmak, yayınlanan paketi temsil eden bir nesne oluşturdu. Şu andan itibaren, Sui client call komutunda paket nesne kimliğini (`0xdbcee02bd4eb326122ced0a8540f15a057d82850`) kullanın ([Move kodu çağırma](https://docs.sui.io/devnet/build/cli-client#calling-move-code) bölümünde yerleşik paketler için kullanılan `0x2'`ye benzer).

Paket yayınlama sonucunda oluşturulan bir diğer nesne, yayınlanan pakete dahil edilen (tek) modülün başlatıcı işlevi içinde oluşturulan kullanıcı tanımlı bir nesnedir (`Forge` türünde) - daha fazla ayrıntı için Move geliştirici belgelerinin [modül başlatıcılarla](https://docs.sui.io/devnet/build/move/debug-publish#module-initializers) ilgili bölümüne bakın.

Yayın için ödeme yapmak üzere kullanılan gas nesnesinin de güncellendiğini fark edebilirsiniz.

**Önemli:** Yayınlama girişimi doğrulama başarısızlığı ile ilgili bir hatayla sonuçlanırsa, daha ayrıntılı bir hata mesajı almak için [paketinizi yerel olarak derleyin](https://docs.sui.io/devnet/build/move/build-test#building-a-package) (`sui move build` komutunu kullanarak).

### Genesis'i özelleştirme <a href="#customize-genesis" id="customize-genesis"></a>

genesis süreci, `--config flag`'i kullanılarak bir genesis yapılandırma dosyası sağlanarak özelleştirilebilir.

```
sui genesis --config <Path to genesis config file>
```

Örnek `genesis.yaml`:

```
---
validator_config_info: ~
committee_size: 4
accounts:
  - gas_objects:
      - object_id: "0xdbac75c4e5a5064875cb8566a533547957092f93"
        gas_value: 100000
    gas_object_ranges: []
move_packages: ["<Paths to custom move packages>"]
sui_framework_lib_path: ~
move_framework_lib_path: ~

```
