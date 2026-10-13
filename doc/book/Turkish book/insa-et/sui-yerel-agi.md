# Sui Yerel Ağı

Yerel ortamınızda bir Sui ağının nasıl oluşturulacağını öğrenin. Yerel ağ ile etkileşim kurmak için [Sui Client CLI](https://docs.sui.io/devnet/build/cli-client)'yı kullanın.

### Sui'yi Kurun <a href="#install-sui" id="install-sui"></a>

Yerel bir Sui ağı oluşturmak için önce Sui'yi yükleyin. [İnşa etmeye başlamak için Sui'yi Yükleme](https://docs.sui.io/devnet/build/install) bölümüne bakın.

### Genesis <a href="#genesis" id="genesis"></a>

Yerel bir Sui ağının yapılandırma dosyalarını ve nesnelerini oluşturmak için `genesis` komutunu çalıştırın. Genesis, ağ yapılandırma dosyalarını \~/.sui/sui\_config klasöründe oluşturur. Bu, fullnode, ağ, istemci ve her doğrulayıcı için bir YAML dosyası içerir. Ayrıca istemci anahtar çiftlerini saklayan bir sui.keystore oluşturur.

Genesis'in oluşturduğu ağ, her biri beş coin nesnesi içeren dört validatör ve beş kullanıcı hesabı içerir.

#### Client CLI'ını kullandıktan sonra genesis'i çalıştırın <a href="#run-genesis-after-using-the-client-cli" id="run-genesis-after-using-the-client-cli"></a>

Yerel bir ağ oluşturmadan önce Sui İstemci CLI'ını kullandıysanız, .sui/sui\_config dizininde bir client.yaml dosyası oluşturur. Yerel bir ağ oluşturmak için genesis'i çalıştırdığınızda, mevcut client.yaml dosyası nedeniyle .sui/sui\_config klasörünün boş olmadığına dair bir uyarı görüntülenir. Yapılandırma dosyalarını değiştirmek için `--force` bağımsız değişkenini kullanabilir veya ağ yapılandırma dosyaları için farklı bir dizin belirtmek üzere `--working-dir` kullanabilirsiniz.

.sui/sui\_config dizinindeki yapılandırma dosyalarını değiştirmek için aşağıdaki komutu kullanın.&#x20;

Yapılandırma dosyalarını depolamak üzere farklı bir dizin kullanmak için aşağıdaki komutu kullanın.

```
sui genesis --working-dir /workspace/config-files
```

Komutu çalıştırmadan önce dizinin zaten var olması ve boş olması gerekir.

**Gömülü ağ geçidi (Embedded gateway)**

Yerel ağınızla birlikte gömülü bir ağ geçidi kullanabilirsiniz. gateway.yaml dosyası gömülü ağ geçidi hakkında bilgi içerir. Gömülü ağ geçidi, Sui'nin gelecekteki bir sürümünde kullanımdan kaldırılacaktır.

### Yerel Ağı Başlatın <a href="#start-the-local-network" id="start-the-local-network"></a>

Yapılandırma için varsayılan konumu kabul ettiğinizi varsayarak yerel Sui ağını başlatmak için aşağıdaki komutu çalıştırın:&#x20;

`sui start`

Bu komut `~/.sui/sui_config` dizininde Sui ağ yapılandırma dosyası `network.yaml`'ı arar. `genesis`'i çalıştırırken farklı bir dizin kullandıysanız, ağı başlattığınızda bu dizinin yolunu belirtmek için `--network.config` bağımsız değişkenini kullanın.&#x20;

Varsayılan dizin dışında bir dizinde bir network.yaml dosyası kullanmak için aşağıdaki komutu kullanın:

```
sui start --network.config /workspace/config-files/network.yaml
```

Ağı başlattığınızda, Sui validator verilerini saklayan bir authorities\_db dizini ve konsensüs verilerini saklayan bir consensus\_db dizini oluşturur.

İşlem tamamlandıktan sonra, yerel ağ ile etkileşim kurmak için [Sui Client CLI](https://docs.sui.io/devnet/build/cli-client)'yi kullanın.
