# Sui Devnet'ine Bağlanın

Sui ile denemeler yapmak için Sui Devnet ağını kullanın. Lütfen Devnet'i kullanma deneyiminiz hakkında geri bildirim gönderin, hataları bildirin ve Sui'ye katkıda bulunun.

Sui Devnet ağı şunlardan oluşmaktadır:

* Mysten Labs tarafından işletilen dört validatör node'u. İstemciler bu uç nokta üzerinden işlem ve okuma istekleri gönderir: `https://fullnode.devnet.sui.io:443` [JSON-RPC](https://docs.sui.io/devnet/build/json-rpc) kullanarak.
* İşlem geçmişine göz atmak için herkese açık bir [Sui Explorer](https://explorer.sui.io/).

[Test SUI tokenlerin](https://docs.sui.io/devnet/build/devnet#request-test-tokens) Sui [devnet-faucet](https://discordapp.com/channels/916379725201563759/971488439931392130) Discord kanalı üzerinden talep edebilirsiniz. Bu tokenlerin hiçbir finansal değeri yoktur. Her Sui sürümünde ağ sıfırlanır ve tüm varlıklar (coinler ve NFT'ler) kaldırılır.

Sui Devnet ile ilgili duyuruları [#devnet-updates](https://discord.com/channels/916379725201563759/1004638487078772736) Discord kanalında görebilirsiniz.

Devnet ağını kullanmak için [hizmet şartlarına](https://sui.io/terms/) bakın.

Sui, Sui Devnet ile etkileşim için aşağıdaki araçları sağlar:

* [Sui komut satırı arayüzü ](https://docs.sui.io/devnet/build/cli-client)(CLI)
  * özel anahtarlarınızı oluşturun ve yönetin
  * örnek NFT'ler oluşturun
  * Move modüllerini çağırma ve yayınlama
* Ağdaki işlemleri ve nesneleri görüntülemek için [Sui Explorer](https://github.com/MystenLabs/sui/blob/main/apps/explorer/README.md)

### Ortam kurulumu <a href="#environment-set-up" id="environment-set-up"></a>

İlk olarak, [Sui'yi yükleyin](https://docs.sui.io/devnet/build/install#sui-tokens). Sui'yi yükledikten sonra [Discord](https://discordapp.com/channels/916379725201563759/971488439931392130) üzerinden [SUI test token'larını](https://docs.sui.io/devnet/build/devnet#request-gas-tokens) talep edin.

Sui'nin zaten kurulu olup olmadığını kontrol etmek için aşağıdaki komutu çalıştırın:

```
which sui
```

Sui yüklüyse, komut Sui binary'sine giden yolu gösterir. Sui yüklü değilse, `sui not found` sonucunu döndürür.

Her Sui sürümündeki değişiklikleri görüntülemek için[ Sui Sürümleri](https://github.com/MystenLabs/sui/releases) sayfasına bakın.

### Sui istemcisini yapılandırma <a href="#configure-sui-client" id="configure-sui-client"></a>

Daha önce yerel bir ağ oluşturmak için `sui genesis -f` komutunu çalıştırdıysanız, `localhost http://0.0.0.0:9000` adresine bağlanan bir Sui istemci yapılandırma dosyası (client.yaml) oluşturdu. Client.yaml dosyasını güncellemek için [Özel RPC uç noktasına bağlan](https://docs.sui.io/devnet/build/devnet#connect-to-custom-rpc-endpoint) bölümüne bakın.

Sui istemcisini Sui Devnet'e bağlamak için aşağıdaki komutu çalıştırın:

```
sui client
```

Sui istemcisini ilk kez başlattığınızda, aşağıdaki mesajı görüntüler:

```
Config file ["/Users/dir/.sui/sui_config/client.yaml"] doesn't exist, do you want to connect to a Sui RPC server [y/n]?
```

**y** tuşuna ve ardından **Enter** tuşuna basın. Daha sonra RPC sunucu URL'sini sorar:

```
Sui RPC server Url (Default to Sui Devnet if not specified) :
```

Sui Devnet'e bağlanmak için **Enter** tuşuna basın. Özel bir RPC sunucusu kullanmak için, kullanılacak RPC uç noktasının URL'sini girin.

```
Select key scheme to generate keypair (0 for ed25519, 1 for secp256k1):
```

Anahtar düzeni seçmek için **0** veya **1** yazın.

**Özel RPC uç noktasına bağlanma**

Daha önce `sui genesis`'i force seçeneği (`-f` veya `--force`) ile kullandıysanız, client.yaml dosyanız zaten iki RPC uç noktası içerir: localnet `http://0.0.0.0:9000` ve devnet `https://fullnode.devnet.sui.io:443`). Tanımlanan ortamları `sui client envs` komutu ile görüntüleyebilir ve `sui client switch` komutu ile bunlar arasında geçiş yapabilirsiniz.

Daha önce Devnet ağına bağlanan bir Sui istemcisi yüklediyseniz veya yerel bir ağ oluşturduysanız, yapılandırılmış RPC uç noktasını değiştirmek için mevcut `client.yaml` dosyanızı değiştirebilirsiniz:

Özel bir RPC uç noktası eklemek için aşağıdaki komutu çalıştırın. `<` `>` içindeki değerleri kurulumunuza uygun değerlerle değiştirin:

```
sui client new-env --alias <ALIAS> --rpc <RPC>
```

Etkin ağı değiştirmek için aşağıdaki komutu çalıştırın:

```
sui client switch --env <ALIAS>
```

Bir sorunla karşılaşırsanız, Sui yapılandırma dizinini (`~/.sui/sui_config`) silin ve en son Sui binary'lerini yeniden yükleyin.

### Validasyon <a href="#validating" id="validating"></a>

Aşağıdaki bölümlerde kullanılan nesne kimliklerinin, adreslerin ve yetki imzalarının yalnızca örnek değerler olduğunu unutmayın. Sui bunların her biri için benzersiz değerler üretir, bu nedenle komutları çalıştırdığınızda farklı değerler görürsünüz.

### Test tokenleri isteme <a href="#request-test-tokens" id="request-test-tokens"></a>

1. [Discord](https://discord.gg/sui)'a Katılın. Yeni oluşturulmuş bir Discord hesabı kullanarak Sui Discord kanalına katılmaya çalışırsanız, doğrulama için birkaç gün beklemeniz gerekebilir.
2.  Sui istemci adresinizi alın:

    ```
    sui client active-address
    ```
3. Sui [#devnet-faucet](https://discord.com/channels/916379725201563759/971488439931392130) Discord kanalında test SUI token'ları talep edin.

Müşteri adresinizle birlikte kanala aşağıdaki mesajı gönderin: !faucet

### Örnek bir NFT mintleyin <a href="#mint-an-example-nft" id="mint-an-example-nft"></a>

Değiştirilebilir Olmayan Token (NFT) oluşturmak için çalıştırın:

```
sui client create-example-nft
```

Komut aşağıdakine benzer bir yanıt verir:

```
Successfully created an ExampleNFT:

ID: ED883F6812AF447B9B0CE220DA5EA9E0F58012FE
Version: 1
Owner: Account Address ( 9E9A9D406961E478AA80F4A6B2B167673F3DF8BA )
Type: 0x2::devnet_nft::DevNetNFT
```

Önceki komut `ED883F6812AF447B9B0CE220DA5EA9E0F58012FE` kimliğine sahip bir nesne oluşturdu. [Adresin sahip olduğu nesneleri görüntülemek](https://docs.sui.io/devnet/build/cli-client#view-objects-owned-by-the-address) için Sui Client CLI'yı kullanın.

Oluşturulan nesneyi [Sui Explorer](https://explorer.sui.io/)'da görüntülemek için, nesne kimliğini aşağıdaki URL'ye ekleyin [https://explorer.sui.io/objects/](https://explorer.sui.io/objects/).

Aşağıdaki komut NFT'nin adının, açıklamasının veya görüntüsünün nasıl özelleştirileceğini göstermektedir:

```
$ sui client create-example-nft --url=https://user-images.githubusercontent.com/76067158/166136286-c60fe70e-b982-4813-932a-0414d0f55cfb.png --description="The greatest chef in the world" --name="Greatest Chef"
```

Komut yeni bir nesne kimliği verir:

```
Successfully created an ExampleNFT:

ID: EC97467A40A1305FFDEF7019C3045FBC7AA31E29
Version: 1
Owner: Account Address ( 9E9A9D406961E478AA80F4A6B2B167673F3DF8BA )
Type: 0x2::devnet_nft::DevNetNFT
```

Nesne hakkındaki ayrıntıları Sui Explorer'da görüntüleyebilirsiniz: [https://explorer.sui.io/objects/EC97467A40A1305FFDEF7019C3045FBC7AA31E29](https://explorer.sui.io/objects/EC97467A40A1305FFDEF7019C3045FBC7AA31E29)

### Bir Move modülü yayınlayın <a href="#publish-a-move-module" id="publish-a-move-module"></a>

Bu bölümde, [Sui Move eğitiminde](https://docs.sui.io/devnet/build/move/write-package) geliştirilen kodu kullanarak örnek bir Move paketinin nasıl yayınlanacağı açıklanmaktadır. Talimatlar, Sui'yi varsayılan konuma yüklediğinizi varsaymaktadır.

```
sui client publish --path <your-sui-repo>/sui_programmability/examples/move_tutorial --gas-budget 30000
```

Yanıt aşağıdakine benzer:

```
----- Certificate ----
Signed Authorities : [k#2266186afd9da10a43dd3ed73d1039c6793d2d8514db6a2407fcf835132e863b, k#1d47ad34e2bc5589882c500345c953b5837e30d6649d315c61690ba7a1e28d23, k#e9599283c0da1ac2eedeb89a56fc49cd8f3c0d8d4ddba9b0a0a5054fe7df3ffd]
Transaction Kind : Publish

----- Publish Results ----
The newly published package object ID: 0689E58788C875E9C354F359792CEC016DA0A1B0
List of objects created by running module initializers:

ID: 898922A9CABE93C6C38C55BBE047BFB0A8C864BF
Version: 1
Owner: Account Address ( F16A5AEDCDF9F2A9C2BD0F077279EC3D5FF0DFEE )
Type: 0x689e58788c875e9c354f359792cec016da0a1b0::my_module::Forge

Updated Gas : Coin { id: 58C4DAA98694266F4DF47BA436CD99659B6A5342, value: 49552 }
```

Paket yayınlama işlemi iki önemli şey yapar:

* Bir paket nesnesi oluşturur (Kimliği `0689E58788C875E9C354F359792CEC016DA0A1B0`)
* Bu paketin bir (ve tek) modülü için bir [modül başlatıcı](https://docs.sui.io/devnet/build/move/debug-publish#module-initializers) çalıştırmanın sonucu olarak bir `forge` nesnesi (`898922A9CABE93C6C38C55BBE047BFB0A8C864BF` kimliğiyle) oluşturur.

Bir paketi yayınladığınızda, oluşturulan nesnelerin kimlikleri bu örnekte görüntülenenlerden farklı olur. Bu konunun geri kalanında, oluşturulan nesnelerin kimliklerini temsil etmek için \<PACKAGE\_ID> ve \<FORGE\_ID> kullanılır. Bunları kendi paketinizin değerleriyle değiştirmelisiniz.

### Bir Move çağırısı (call) yapın <a href="#make-a-move-call" id="make-a-move-call"></a>

Bu bölüm, önceki bölümde yayınlanan pakette tanımlanan işlevlerin nasıl çağrılacağını açıklamaktadır. Kılıçları oluşturmak ve diğer oyunculara aktarmak için paketinizdeki (\<PACKAGE\_ID>) ve (\<FORGE\_ID>) değerlerini kullanın.

Bunu göstermek için, \<PLAYER\_ADDRESS> kılıç alacak oyuncunun adresini temsil eder. Tanıdığınız birinin adresini kullanabilir veya aşağıdaki Sui Client CLI komutu ile test için başka bir adres oluşturabilirsiniz:

```
sui client new-address ed25519
```

Komut aşağıdaki mesajı ve adres için 12 kelimelik bir kurtarma cümlesini verir:

```
Created new keypair for address with scheme Secp256k1: [0x568318261d88535009dff39779b18e1bfac59c33]
Secret Recovery Phrase : [mist drizzle rain shower downpour pond stream brook river ocean sea suinami]
```

Bir kılıç oluşturmak ve onu başka bir oyuncuya aktarmak için, daha önce yayınladığımız paketin `my_module` [modülündeki ](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/move\_tutorial/sources/my\_module.move#L4)`sword_create` [fonksiyonunu ](https://github.com/MystenLabs/sui/blob/main/sui\_programmability/examples/move\_tutorial/sources/my\_module.move#L47)çağırmak için aşağıdaki komutu kullanıyoruz.

Komut ve işlev parametreleri için gösterilen örnekle aynı biçimi kullanmalısınız.

```
sui client call --function sword_create --module my_module --package 0x<PACKAGE_ID> --args \"0x<FORGE_ID>\" 42 7 \"0x<PLAYER_ADDRESS>\" --gas-budget 30000
```

Yanıt aşağıdakine benzer:

```
----- Certificate ----
Signed Authorities : [k#2266186afd9da10a43dd3ed73d1039c6793d2d8514db6a2407fcf835132e863b, k#1d47ad34e2bc5589882c500345c953b5837e30d6649d315c61690ba7a1e28d23, k#e9599283c0da1ac2eedeb89a56fc49cd8f3c0d8d4ddba9b0a0a5054fe7df3ffd]
Transaction Kind : Call
Package ID : 0x689e58788c875e9c354f359792cec016da0a1b0
Module : my_module
Function : sword_create
Arguments : [ImmOrOwnedObject((898922A9CABE93C6C38C55BBE047BFB0A8C864BF, SequenceNumber(1), o#9f12d4390e4fc8de3834c4960c6f265a78eca7c2b916ac1be66c1f00e1b47c68)), Pure([42, 0, 0, 0, 0, 0, 0, 0]), Pure([7, 0, 0, 0, 0, 0, 0, 0]), Pure([45, 50, 237, 113, 56, 27, 239, 127, 61, 140, 87, 180, 141, 248, 33, 35, 89, 54, 114, 170])]
Type Arguments : []

----- Transaction Effects ----
Status : Success { gas_cost: GasCostSummary { computation_cost: 69, storage_cost: 40, storage_rebate: 27 } }
Created Objects:
  - ID: 2E34983D59E9FC5310CFBAA953D2188E6A84FD21 , Owner: Account Address ( 2D32ED71381BEF7F3D8C57B48DF82123593672AA )
Mutated Objects:
  - ID: 58C4DAA98694266F4DF47BA436CD99659B6A5342 , Owner: Account Address ( ADE6EAD34629411F730416D6AD48F6B382BBC6FD )
  - ID: 898922A9CABE93C6C38C55BBE047BFB0A8C864BF , Owner: Account Address ( ADE6EAD34629411F730416D6AD48F6B382BBC6FD )
```

Yeni oluşturulan bir nesneyi gözlemlemek için Sui Explorer'a gidin. `Magic` özelliği `42` ve `Strength` özelliği `7` ile oluşturulmuş ve yeni sahibine aktarılmış bir kılıç nesnesi görmelisiniz.

Explorer'daki nesne kimliğini, kendi komut çıktınızda gözlemlediğiniz oluşturulan nesnenin nesne kimliği ile değiştirin, ek olarak: [https://explorer.sui.io/objects/](https://explorer.sui.io/objects/)

İlgili konular:

* Move ile Akıllı Kontratlar Oluşturun.
* Nesnelerle Programlama
