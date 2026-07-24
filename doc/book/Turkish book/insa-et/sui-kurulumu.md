# Sui Kurulumu

Sui blockchain üzerinde akıllı kontratlar geliştirmek için Sui'nin nasıl kurulacağını ve yapılandırılacağını öğrenin.

Sui'yi yüklemeden önce bazı önkoşul araçları yüklemeniz ve geliştirme ortamınızı yapılandırmanız gerekir.

Sui'yi yüklemek için gereken adımlar şunlardır:

1. İşletim sisteminiz için [ön gereksinimleri](https://docs.sui.io/devnet/build/install#prerequisites) yükleyin.
2. [Sui binary'lerini](https://docs.sui.io/devnet/build/install#install-sui-binaries) yükleyin.
3. Bir [Entegre Geliştirme Ortamı (IDE)](https://docs.sui.io/devnet/build/install#integrated-development-environment) yapılandırın.
4. Devnet ve Sui Wallet'ı değerlendirmek için [SUI tokenları ](https://docs.sui.io/devnet/build/install#sui-tokens)talep edin.
5. İsteğe bağlı olarak, örneklere yerel olarak erişmek ve Sui'ye katkıda bulunmak için [kaynak kodunu](https://docs.sui.io/devnet/build/install#source-code) indirin.

### Sui deposu (repository) <a href="#sui-repository" id="sui-repository"></a>

Sui deposu`devnet` ve `main` olmak üzere iki ana dal içermektedir.

* `devnet` dalı, Sui'nin en son kararlı yapısını içerir. Sui üzerinde derleme veya test yapmak istiyorsanız `devnet` dalını seçin. Bir sorunla karşılaşırsanız veya bir hata bulursanız, `main` dalda zaten düzeltilmiş olabilir. Bir çekme isteği (Pull Request - PR) göndermek için, `main` dalın çatalına taahhütler göndermelisiniz.
* `main` dal en son değişiklikleri ve güncellemeleri içerir. Sui projesine katkıda bulunmak istiyorsanız `main` dalı kullanın. `main` dal yayınlanmamış değişiklikler içerebilir veya daha önceki bir sürüm kullanılarak oluşturulan uygulamalarda sorunlara neden olan değişiklikler içerebilir.

### Sui deposundaki dokümantasyon <a href="#documentation-in-the-sui-repository" id="documentation-in-the-sui-repository"></a>

Sui deposunun `main` ve `devnet` dalları, her dal için ilgili belgeleri içerir. Dokümantasyon sitesindeki bir sürüm geçişi, `main dal` içeriği (Latest build etiketli) ve `devnet` dalı içeriği (Devnet etiketli) arasında geçiş yapmanızı sağlar. Sui'nin nasıl kurulacağını, yapılandırılacağını ve derleneceğini öğrenmek için geçişin Devnet olarak ayarlandığından emin olun. Son sürümdeki içerik, Sui'deki olası güncellemeler hakkında bilgi edinmek için kullanışlıdır, ancak açıklanan özellikler ve işlevler `devnet` dalında hiçbir zaman kullanılamayabilir.

### Desteklenen işletim sistemleri <a href="#supported-operating-systems" id="supported-operating-systems"></a>

Sui, belirtilen sürümlerden başlayarak aşağıdaki işletim sistemlerini destekler.

* Linux - Ubuntu versiyon 20.04 (Bionic Beaver)
* macOS - macOS Monterey
* Microsoft Windows - Windows 11

### Öngereksinimler <a href="#prerequisites" id="prerequisites"></a>

Sui ile çalışmak için ihtiyacınız olan önkoşulları ve araçları yükleyin. İlgili bölüme atlamak için tablodaki bir işarete tıklayın.

| Paket/OS        | Linux                 | macOS                 | Windows 11            |
| --------------- | --------------------- | --------------------- | --------------------- |
| cURL            | [X](broken-reference) | [X](broken-reference) | [X](broken-reference) |
| Rust ve Cargo   | [X](broken-reference) | [X](broken-reference) | [X](broken-reference) |
| Git CLI         | [X](broken-reference) | [X](broken-reference) | [X](broken-reference) |
| CMake           | [X](broken-reference) | [X](broken-reference) | [X](broken-reference) |
| libssl-dev      | [X](broken-reference) |                       |                       |
| libclang-dev    | [X](broken-reference) |                       |                       |
| Brew            |                       | [X](broken-reference) |                       |
| C++ build tools |                       |                       | [X](broken-reference) |
| LLVM Compiler   |                       |                       | [X](broken-reference) |

#### Rust ve Cargo <a href="#rust-and-cargo" id="rust-and-cargo"></a>

Sui, desteklenen tüm işletim sistemlerinde Rust ve Cargo gerektirir. Bazı işletim sistemleri Rust ve Cargo'yu indirmek için cURL gerektirir, bu nedenle gerekirse önce cURL'yi yüklemek için ilgili önkoşul bölümünü kontrol edin.

Rust ve Cargo'yu macOS veya Linux'a yüklemek için aşağıdaki komutu kullanın:

```
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

Windows 11 kullanıcıları Rust web sitesinde bulunan [Rust yükleyicisini](https://www.rust-lang.org/tools/install) kullanabilirler. Yükleyici, gerekli C++ derleme araçlarına sahip olup olmadığınızı algılar ve sizden yüklemenizi ister. Ortamınızı en iyi tanımlayan seçeneği seçin ve kurulum sihirbazındaki talimatları izleyin.

Ek kurulum seçenekleri için Rust web sitesindeki [Rust'ı Kur](https://www.rust-lang.org/tools/install) bölümüne bakın.

Sui, bağımlılıkları derlemek ve yönetmek için Cargo'nun en son sürümünü kullanır. Daha fazla bilgi için Rust web sitesindeki [Cargo kurulumu](https://doc.rust-lang.org/cargo/getting-started/installation.html) sayfasına bakın.

Rust'ı güncellemek için aşağıdaki komutu kullanın:

Rust'ı yükledikten sonra, işletim sisteminiz için ön koşullara geçin.

* [Linux öngereksinimleri](https://docs.sui.io/devnet/build/install#linux-prerequisites)
* [macOS öngereksinimleri](https://docs.sui.io/devnet/build/install#macos-prerequisites)
* [Windows öngereksinimleri](https://docs.sui.io/devnet/build/install#windows-prerequisites)

### Linux öngereksinimleri <a href="#linux-prerequisites" id="linux-prerequisites"></a>

> **Not**: Linux talimatları APT paket yöneticisini kullanan bir dağıtımı varsaymaktadır. Talimatları diğer paket yöneticileri için gerektiği gibi uyarlayın.

Bu bölümde listelenen önkoşulları yükleyin. Sisteminizin `apt-get`'in en son sürümüne sahip olduğundan emin olmalısınız. `apt-get`'i güncellemek için aşağıdaki komutu kullanın:

```
sudo apt-get update
```

#### cURL <a href="#curl" id="curl"></a>

Aşağıdaki komut ile cURL'yi yükleyin:

```
sudo apt install curl
```

Aşağıdaki komutla cURL'ün doğru şekilde yüklendiğini doğrulayın:

```
sudo apt install curl
```

#### Git CLI <a href="#git-cli" id="git-cli"></a>

[Git CLI](https://cli.github.com/)'ı da dahil olmak üzere Git'i yüklemek için aşağıdaki komutu çalıştırın:

```
sudo apt-get install git-all
```

Daha fazla bilgi için GitHub web sitesindeki Linux'ta Git Yükle bölümüne bakın.

#### CMake <a href="#cmake" id="cmake"></a>

CMake web sitesindeki [CMake'in Kur](https://cmake.org/install/) bölümündeki talimatları kullanarak CMake'i kurun.

#### libssl-dev <a href="#libssl-dev" id="libssl-dev"></a>

`libssl-dev`'i yüklemek için aşağıdaki komutu kullanın:

```
sudo apt-get install libssl-dev
```

#### libclang-dev <a href="#libclang-dev" id="libclang-dev"></a>

`libclang-dev`'i yüklemek için aşağıdaki komutu kullanın:

```
sudo apt-get install libclang-dev
```

Sui'yi yüklemeye devam etmek için[ Sui binary'lerini](https://docs.sui.io/devnet/build/install#binaries) yükle kısmına ilerleyin.

### macOS öngereksinimleri <a href="#macos-prerequisites" id="macos-prerequisites"></a>

macOS, Brew'u yüklemek için kullanabileceğiniz bir cURL sürümü içerir. Daha yeni bir cURL sürümü de dahil olmak üzere diğer araçları yüklemek için Brew'u kullanın.

#### Brew <a href="#brew" id="brew"></a>

[Brew](https://brew.sh/)'i yüklemek için aşağıdaki komutu kullanın:

```
/bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
```

#### cURL <a href="#curl-1" id="curl-1"></a>

macOS'ta varsayılan [cURL](https://curl.se/)'yi güncellemek için aşağıdaki komutu kullanın:

```
brew install curl
```

#### CMake <a href="#cmake-1" id="cmake-1"></a>

CMake'i yüklemek için aşağıdaki komutu kullanın:

```
brew install cmake
```

#### Git CLI'ı <a href="#git-cli-1" id="git-cli-1"></a>

Git'i yüklemek için aşağıdaki komutu kullanın:

```
brew install git
```

Git'i kurduktan sonra, [Git komut satırı arayüzünü](https://git-scm.com/download/) indirin ve kurun.

Sui'yi yüklemeye devam etmek için [Sui binary'lerini yükle](https://docs.sui.io/devnet/build/install#install-sui-binaries) kısmına ilerleyin.

### Windows öngereksinimleri <a href="#windows-prerequisites" id="windows-prerequisites"></a>

Microsoft Windows 11'de Sui ile çalışmak için aşağıdaki önkoşulları yükleyin.

#### cURL <a href="#curl-2" id="curl-2"></a>

Windows 11'de, [cURL](https://curl.se/windows/microsoft.html)'nin Microsoft sürümü zaten yüklü olarak gelir. Bunun yerine curl proje sürümünü kullanmak istiyorsanız,[ https://curl.se/windows/](https://curl.se/windows/) adresinden indirip yükleyin.

#### Git CLI <a href="#git-cli-2" id="git-cli-2"></a>

[Git komut satırı arayüzünü](https://git-scm.com/download/) indirin ve kurun.

#### CMake <a href="#cmake-2" id="cmake-2"></a>

CMake web sitesinden [CMake](https://cmake.org/download/)'i indirin ve kurun.

#### Protocol Buffers <a href="#protocol-buffers" id="protocol-buffers"></a>

[Protocol Buffers](https://github.com/protocolbuffers/protobuf/releases) (protoc-xx.x-win32.zip veya protoc-xx.x-win64.zip) dosyasını indirin ve \bin dizinini Windows PATH ortam değişkeninize ekleyin.

#### Windows için ek araçlar <a href="#additional-tools-for-windows" id="additional-tools-for-windows"></a>

Sui, Windows çalıştıran bilgisayarlarda aşağıdaki ek araçları gerektirir.

* Yalnızca Windows'un ARM64 sürümü için - [Visual Studio 2022 Önizlemesi](https://visualstudio.microsoft.com/vs/preview/).
* [Rust'ı yüklemek](https://docs.sui.io/devnet/build/install#rust-and-cargo) için [C++ derleme araçları](https://visualstudio.microsoft.com/downloads/) gereklidir, bu nedenle bu talimatları izlediyseniz bunları zaten yüklemiş olmanız gerekir.
* [LLVM Derleyici Altyapısı](https://releases.llvm.org/).

İlerleme donmuş gibi görünüyorsa, sorunu çözmek için komut isteminde **Enter** tuşuna basın.

**Bilinen Problem** - `sui console` komutu PowerShell'de çalışmıyor.

### Sui binary'lerini kurun <a href="#install-sui-binaries" id="install-sui-binaries"></a>

Cargo yüklüyken, Sui binary'lerini yüklemek için aşağıdaki komutu kullanın:

```
cargo install --locked --git https://github.com/MystenLabs/sui.git --branch devnet sui
```

Komut aşağıdaki Sui bileşenlerini `~/.cargo/bin` içine yükler:

* `sui` - Sui CLI aracı, validatörlerin ve hesapların `genesis`'ini (ilk kez oluşumunu) sağlamak, Sui ağını başlatmak ve Move paketlerini oluşturmak ve test etmek için alt komutların yanı sıra Sui ağıyla etkileşim için bir istemci içerir.
* Önceki komut başarısız olursa, Rust'ın en son sürümünün yüklü olduğundan emin olun:

```
rustup update stable
source "$HOME/.cargo/env"
```

#### macOS ve Linux <a href="#macos-and-linux" id="macos-and-linux"></a>

İkili dosyaların PATH değişkeninizde görünen `.cargo/bin` ile yüklendiğini onaylayın:

```
echo $PATH
```

#### Windows <a href="#windows" id="windows"></a>

İkili dosyaların PATH değişkeninizde görünen `.cargo\bin` ile yüklendiğini onaylayın:

```
echo %PATH%
```

Bu ikililerden herhangi biri için yararlı bilgilere erişmek için `--help` flag'ini kullanın.

**Önemli** - Tüm araç zincirinizin güncel kaldığından emin olun. Sui ikililerini oluştururken ve yüklerken sorunlarla karşılaşırsanız, tüm paketleri güncelleyin ve yeniden yükleyin.

### Entegre Geliştirme Ortamı <a href="#integrated-development-environment" id="integrated-development-environment"></a>

Move geliştirme için önerilen IDE, move-analyzer uzantısına sahip [Visual Studio Code](https://code.visualstudio.com/)'dur. [move-nalyzer uzantısını](https://marketplace.visualstudio.com/items?itemName=move.move-analyzer) yüklemek için Visual Studio Marketplace talimatlarını izleyin ve `--features` flag'ini kullanarak `address20` adresini geçin:

```
cargo install --git https://github.com/move-language/move move-analyzer --features "address20"
```

[Awesome Move](https://github.com/MystenLabs/awesome-move) belgelerinde daha fazla [IDE](https://github.com/MystenLabs/awesome-move#ides) seçeneğine bakın.

### SUI tokenleri <a href="#sui-tokens" id="sui-tokens"></a>

[Devnet ile deneme yapmak](https://docs.sui.io/devnet/build/devnet) veya [Sui Cüzdan'ı tarayıcı uzantısını kullanmak](https://docs.sui.io/devnet/explore/wallet-browser) için hesap adresinize SUI token'ları ekleyin.

Tarayıcı uzantısından SUI test token'ları talep etmek için:

1. Cüzdanınızı açmak için Sui Cüzdan tarayıcı uzantısını kullanın.
2. **Sui Devnet Jetonlarını Talep E**t'e tıklayın.

Discord'da SUI test tokenları talep etmek için:

1. Henüz katılmadıysanız [Sui Discord](https://discord.com/invite/sui)'una katılın.
2.  Adresinizi Sui Cüzdanı tarayıcı uzantısı aracılığıyla veya aşağıdaki komutu çalıştırarak ve istenirse bir Sui RPC sunucusuna bağlanmayı seçerek belirleyin:

    ```
    sui client active-address
    ```
3.  Sözdizimini kullanarak [#devnet-faucet](https://discord.com/channels/916379725201563759/971488439931392130) kanalında belirteçler isteyin: `!faucet <YOUR_ADDRESS>`, örneğin:

    ```
    !faucet 0xd72c2c90ed9d923cb0ed2ca91db5be9e1c9b5ccb
    ```
4. Kanaldaki bir bot tokenları size otomatik olarak dağıtır.

### Kaynak kodu <a href="#source-code" id="source-code"></a>

GitHub'da Sui deposunu görüntüleyin: [https://github.com/MystenLabs/sui](https://github.com/MystenLabs/sui)

Sui deposunu klonlayın:

```
git clone https://github.com/MystenLabs/sui.git --branch devnet
```

Aşağıdaki ana dizinler Sui'nin kaynak kodunu keşfetmek için iyi bir başlangıç noktası sunar:

* [sui](https://github.com/MystenLabs/sui/tree/main/crates/sui) - Sui CLI binary'si
* [sui\_programmability](https://github.com/MystenLabs/sui/tree/main/sui\_programmability) - Sui'nin Move dili entegrasyonu, test ve yeniden kullanım için oyunlar ve diğer Move kodu örnekleri de dahil
* [sui\_core](https://github.com/MystenLabs/sui/tree/main/crates/sui-core) - otorite sunucusu ve Sui Ağ Geçidi
* [sui-types](https://github.com/MystenLabs/sui/tree/main/crates/sui-types) - coinler, gas ve diğer nesne türleri
* [explorer](https://github.com/MystenLabs/sui/tree/main/apps/explorer) - Sui ağı için nesne gezgini
* [sui-network](https://github.com/MystenLabs/sui/tree/main/crates/sui-network) - ağ arayüzleri

### Rustdoc <a href="#rustdoc" id="rustdoc"></a>

Kullanımdaki Rust Crate'lerini şu adresten görebilirsiniz:

* [https://mystenlabs.github.io/sui/](https://mystenlabs.github.io/sui/) - Sui blockchain'i
* [https://mystenlabs.github.io/narwhal/](https://mystenlabs.github.io/narwhal/) - Narwhal and Bullshark konsensüs motoru
* [https://mystenlabs.github.io/mysten-infra/](https://mystenlabs.github.io/mysten-infra/) - Mysten Labs altyapısı

### Yardım <a href="#help" id="help"></a>

Sui kodundaki güncellemelere katkıda bulunmak için çekme istekleri (pull requests) gönderin.

> **Not:** Önceki `git clone` komutu, kaynak kodun `devnet`'imizle uyumlu olmasını sağlayan `devnet` dalıyla senkronize olur. En son sürümü kullanarak yerel olarak bir ağ çalıştırmak istiyorsanız ve `devnet`'imizle etkileşime girmeniz gerekmiyorsa, `main` dala geçmelisiniz.

### Sonraki adımlar <a href="#next-steps" id="next-steps"></a>

Yolculuğunuza devam edin:

* Move ile Akıllı Kontratlar
* Sui istemci Hızlı Başlangıç'ı
* RPC Server API'ı
* End-to-End (baştan sona) eğitim
