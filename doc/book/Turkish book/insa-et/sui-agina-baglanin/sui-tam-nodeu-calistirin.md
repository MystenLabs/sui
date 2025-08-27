# Sui Tam node'u Çalıştırın

Sui Tam node'lar işlemler, kontrol noktaları ve dönem değişiklikleri dahil olmak üzere blok zinciri faaliyetlerini doğrular. Her bir Tam düğüm, blok zinciri durumu ve geçmişi için sorguları depolar ve hizmet verir.

Bu rol, [validatörlerin](https://docs.sui.io/devnet/learn/architecture/validators) işlemlere hizmet vermeye ve işlemeye odaklanmasını sağlar. Bir validatör yeni bir işlem setini (ya da bir işlem bloğunu) işleme koyduğunda, validatör bu bloğu tüm bağlı Tam node'lara gönderir ve bunlar da müşterilerden gelen sorgulara hizmet verir.

### Özellikler <a href="#features" id="features"></a>

Sui Tam node'lar:

* Blok zincirinin durumunu bağımsız ve yerel olarak takip edip ve doğrulayabilir
* Müşterilerden gelen okuma taleplerini karşılayabilir

### Durum senkronizasyonu <a href="#state-synchronization" id="state-synchronization"></a>

Sui Tam node'ları ağdaki yeni işlemleri almak için doğrulayıcılarla senkronize olur.

Bir işlem, bir işlem sertifikası (TxCert) oluşturmak için 2f + 1 validatöre birkaç gidiş dönüş gerektirir.

Bu senkronizasyon süreci şunları içerir:

1. 2f+1 validatörlerini takip etme ve yeni işlenen işlemleri dinleme
2. 2f+1 validatörlerinin işlemi tanıdığından ve kesinliğe ulaştığından emin olma.
3. İşlemin yerel olarak yürütülmesi ve yerel DB'nin güncellenmesi.

Bu senkronizasyon süreci, bir Tam node'un tüm yeni işlemleri düzgün bir şekilde işlediğinden emin olmak için en az 2f+1 validatörün dinlenmesini gerektirir. Sui, kontrol noktalarının ve diğer Tam node'larla senkronizasyon yeteneğinin tanıtılmasıyla senkronizasyon sürecini geliştirecektir.

### Mimari

Bir Sui Tam node esasen ağ durumunun salt okunur bir görünümüdür. Doğrulayıcı node'lerin aksine, tam node'ler işlemleri imzalayamazlar, ancak daha önce bir validatör grubu tarafından gerçekleştirilmiş olan işlemleri yeniden gerçekleştirerek zincirin bütünlüğünü doğrulayabilirler.

Günümüzde bir Sui Tam node'u zincirin tüm geçmişini saklar.

Validatör node'lar yalnızca nesne grafiğinin sınırındaki en son işlemleri saklar (örneğin, >0 harcanmamış çıktı nesnesine sahip işlemler).

### Tam node kurulumu

Kendi Sui Tam node'unuzu çalıştırmak için buradaki talimatları izleyin.

#### Donanım gereksinimleri

Bir Sui Tam node'u çalıştırmak için önerilen donanım gereksinimleri:

* CPU: 10 core
* RAM: 32 GB
* Depolama Alanı: 1 TB

#### Sistem gereksinimleri <a href="#software-requirements" id="software-requirements"></a>

Sui Tam node'larını Linux üzerinde çalıştırmanızı öneririz. Sui, Ubuntu ve Debian dağıtımlarını desteklemektedir. MacOS üzerinde de bir Sui Tam node çalıştırabilirsiniz.

[Rust](https://docs.sui.io/devnet/build/install#rust)'ı güncellediğinizden emin olun.

Ek Linux gereksinimlerini yüklemek için aşağıdaki komutu kullanın.

```
    $ apt-get update \
    && apt-get install -y --no-install-recommends \
    tzdata \
    ca-certificates \
    build-essential \
    pkg-config \
    cmake
```

### Tam Node'u Yapılandırın <a href="#configure-a-full-node" id="configure-a-full-node"></a>

Bir Sui Tam node'nu Docker kullanarak ya da kaynaktan oluşturarak yapılandırabilirsiniz.

#### Docker Compose'u Kullanımı <a href="#using-docker-compose" id="using-docker-compose"></a>

Docker kullanarak bir Sui Tam node'unu çalıştırmak için [ortamı sıfırlamak](https://github.com/MystenLabs/sui/tree/main/docker/fullnode#reset-the-environment) da dahil olmak üzere [Tam node Docker README'deki](https://github.com/MystenLabs/sui/tree/main/docker/fullnode#readme) talimatları izleyin.

#### Kaynaktan Oluşturma <a href="#building-from-source" id="building-from-source"></a>

* Gerekli [Önkoşulları](https://docs.sui.io/devnet/build/install#prerequisites) yükleyin.
* Sui deposunun fork'unu kurun:
  * GitHub'daki [Sui deposuna](https://github.com/MystenLabs/sui) gidin ve ekranın sağ üst köşesindeki Çatal simgesine tıklayın.
  *   Sui deposunun kişisel çatalını yerel makinenize klonlayın (GitHub kullanıcı adınızı URL'ye eklediğinizden emin olun):

      ```
      $ git clone https://github.com/<YOUR-GITHUB-USERNAME>/sui.git
      ```
* `sui` deponuza `cd` ile girin:

```
cd sui
```

*   Sui deposunu git remote olarak ayarlayın:

    ```
    git remote add upstream https://github.com/MystenLabs/sui
    ```
* Fork'unuzu senkronize edin:

```
git fetch upstream
```

*   Devnet `şubesine` göz atın:

    ```
    git checkout --track upstream/devnet
    ```
*   [Tam node YAML](https://github.com/MystenLabs/sui/blob/main/crates/sui-config/data/fullnode-template.yaml) şablonunun bir kopyasını oluşturun:

    ```
    cp crates/sui-config/data/fullnode-template.yaml fullnode.yaml
    ```
*   Devnet için [`genesis`](https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob) durumunu indirin:

    ```
    curl -fLJO https://github.com/MystenLabs/sui-genesis/raw/main/devnet/genesis.blob
    ```
* İsteğe bağlı: Kaynaklara giden varsayılan yolları kabul etmek için bu adımı atlayın. Özel yolları kullanmak için `fullnode.yaml` dosyasını düzenleyin.
  *   `db-path` alanını Full node veritabanının yolu ile güncelleyin.

      ```
      db-path: "/db-files/sui-fullnode"
      ```
  *   `genesis-file-location` öğesini `genesis.blob` öğesinin yolu ile güncelleyin.

      ```
      genesis:
      genesis-file-location: "/sui-fullnode/genesis.blob"
      ```
*   Sui Tam node'unuzu başlatın:

    ```
    cargo run --release --bin sui-node -- --config-path fullnode.yaml
    ```
* İsteğe bağlı: Websocket üzerinden JSON-RPC kullanarak bildirimleri [yayınlayın / abone](https://docs.sui.io/devnet/build/event\_api#subscribe-to-sui-events) olun.

Tam node'unuz artık [Sui JSON-RPC API](https://docs.sui.io/devnet/build/json-rpc#sui-json-rpc-api)'sinin okuma uç noktalarına şu adresten hizmet verecektir: http://127.0.0.1:9000

### Tam node'unuz ile Sui Gezgini

[Sui Gezgini](https://explorer.sui.io/), özel RPC URL'lerine ve yerel ağlara bağlantıları destekler. Gezgini yerel Tam node'unuza yönlendirebilir ve ağdan senkronize ettiği işlemleri görebilirsiniz. Bu değişikliği yapmak için:

1. Bir tarayıcı açın ve şu adrese gidin: https://explorer.sui.io/
2. Sui Gezgininin sağ üst köşesindeki **Devnet** düğmesine tıklayın ve açılır menüden **Local** (Yerel) öğesini seçin.
3. En son işlemleri görmek için **Ağ Seç** menüsünü kapatın.

Sui Gezgini artık zincirin durumunu keşfetmek için yerel Tam node'unuzu kullanıyor.

### İzleme <a href="#monitoring" id="monitoring"></a>

[Günlük Tutma, İzleme, Ölçümler ve Gözlenebilirlik](https://docs.sui.io/devnet/contribute/observability) bölümündeki talimatları kullanarak Tam node'unuzu izleyin.

Varsayılan metrik bağlantı noktasının 9184 olduğunu unutmayın. Bağlantı noktasını değiştirmek için `fullnode.yaml` dosyanızı düzenleyin.

### Tam node'unuzu güncelleyin

Sui yeni bir sürüm yayınladığında, Devnet veri içermeyen yeni bir ağ olarak yeniden başlar. Ağ ile uyumluluğu sağlamak için Full node'unuzu her Sui sürümünde güncellemeniz gerekir.

### Docker Compose ile Güncelleme

[Ortamı sıfırlamak](https://github.com/MystenLabs/sui/tree/main/docker/fullnode#reset-the-environment) için talimatları izleyin, yani komutu çalıştırarak:

```
docker-compose down --volumes
```

#### Kaynaktan Güncelleme <a href="#update-from-source" id="update-from-source"></a>

[Kaynaktan Oluşturma](https://docs.sui.io/devnet/build/fullnode#building-from-source) talimatlarını izlediyseniz, Tam node'unuzu aşağıdaki gibi güncelleyin:

* Çalışmakta olan Full node'unuzu kapatın.
* `cd` ile yerel Sui deponuza girin:

```
cd sui
```

* Disk üzerindeki eski veritabanını ve 'genesis.blob' dosyasını kaldırın:

```
rm -r suidb genesis.blob
```

* Kaynağı en son sürümden getirin:

```
git fetch upstream
```

*   Şubenizi sıfırlayın:

    ```
    git checkout -B devnet --track upstream/devnet
    ```
* Yukarıda açıklandığı gibi Devnet için en son `genesis` durumunu indirin.
* Gerekirse `fullnode.yaml` yapılandırma dosyanızı güncelleyin.
*   Sui Full node'unuzu yeniden başlatın:

    ```
    cargo run --release --bin sui-node -- --config-path fullnode.yaml
    ```

Tam node'unuz `http://127.0.0.1:9000` 'da başlar.
