# Katkıda Bulunun

Bu sayfada, Sui'ye nasıl katkıda bulunulacağı açıklanmakta ve Sui topluluğuna katılım hakkında ek bilgiler verilmektedir.

Sık sorulan soruların yanıtlarını [SSS ](https://docs.sui.io/devnet/contribute/faq)bölümümüzde bulabilirsiniz.

### Yol haritamıza bakın <a href="#see-our-roadmap" id="see-our-roadmap"></a>

Sui hızla gelişiyor. Önümüzdeki 30 gün içinde planlanan güncellemeler için [Geliştirici Deneyimi Yol Haritamıza](https://github.com/MystenLabs/sui/blob/main/DEVX\_ROADMAP.md) bakın.

Sui topluluğuyla bağlantı kurmak için [Discord'umuza](https://discord.gg/sui) katılın.

### Açık noktalar <a href="#open-issues" id="open-issues"></a>

Sui ile ilgili bir sorunu bildirmek için GitHub reposunda bir [sorun oluşturun](https://github.com/MystenLabs/sui/issues/new/choose). Oluşturulacak sorun türü için bir şablon açmak üzere **Başlayın'a** tıklayın.

### Dokümanlarda güncellemeler <a href="#updates-to-docs" id="updates-to-docs"></a>

Belirli bir konuda güncelleme talebinde bulunmak için sayfanın alt kısmındaki Kaynak Kodu bağlantısına tıklayarak GitHub reposundaki kaynak dosyayı açın. Bir istek göndermek için öncelikle doküman sitesinin **En son derleme sürümünü** seçin. Bu, konunun **Devnet'tekinden** daha yeni bir sürümünü içerebilecek olan reponun ana dalını açar.

**Bu dosyayı düzenle'ye tıklayın**, değişikliklerinizi yapın ve ardından değişikliklerinizi yeni bir dalda içeren bir çekme isteği oluşturmak için **Değişiklik öner'e** tıklayın.

### Katkıda bulunmak için Sui'yi yükleyin <a href="#install-sui-to-contribute" id="install-sui-to-contribute"></a>

Sui kaynak koduna veya belgelerine katkıda bulunmak için yalnızca bir GitHub hesabına ihtiyacınız vardır. Güncellemeleri işleyebilir ve ardından doğrudan Github web sitesinden bir PR gönderebilir veya yerel ortamınızda reponun bir çatalını oluşturabilir ve değişiklik yapmak için favori araçlarınızı kullanabilirsiniz. PR'leri her zaman `main` dalına gönderin.

#### Bir fork oluşturun <a href="#create-a-fork" id="create-a-fork"></a>

Öncelikle, kopyanızla çalışabilmek için hesabınızda Mysten Labs Sui reposunun bir fork'unu oluşturun.

**Web sitesini kullanarak bir çatal oluşturmak için**

1. Github hesabınıza giriş yapın.
2. GitHub'daki [Sui reposuna](https://github.com/MystenLabs/sui) göz atın.
3. Sağ üstte **fork'u** seçin, ardından **Yeni fork oluştur'u** seçin.
4. **Sahip** için kullanıcı adınızı seçin.
5. **Repo adı için** sui adını korumanızı öneririz, ancak herhangi bir ad kullanabilirsiniz.
6. İsteğe bağlı. Katkıda bulunmak için yalnızca reponun ana dalına ihtiyacınız vardır. Tüm dalları dahil etmek için ise **yalnızca `main` dalı kopyalama** (copy the main branch only) onay kutusunun işaretini kaldırın.
7. Fork oluştura basın

#### Fork'unuzu kopyalayın <a href="#clone-your-fork" id="clone-your-fork"></a>

Ardından, reponun fork'unu yerel çalışma alanınıza klonlayın.

**Fork'unuzu yerel çalışma alanınıza klonlamak için**

1. Repo fork'unuz için GitHub sayfasını açın, ardından **fork'u senkronize et'e** tıklayın.
2. **Kod'a** tıklayın, ardından **HTTPS'ye** tıklayın ve görüntülenen web URL'sini kopyalayın.
3. Bir terminal oturumu açın ve kullanılacak klasöre gidin, ardından URL'yi Git sayfasından kopyaladığınız URL ile değiştirerek aşağıdaki komutu çalıştırın:

`git clone https://github.com/github-user-name/sui.git`

Repo otomatik olarak çalışma alanınızdaki sui klasörüne klonlanır. Aşağıdaki komutla çatalınızın bir dalını oluşturun [(veya dallanma ile ilgili GitHub konusunu](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/proposing-changes-to-your-work-with-pull-requests/creating-and-deleting-branches-within-your-repository) izleyin)

`git checkout -b your-branch-name`

&#x20;[remote upstream reposunu](https://docs.github.com/en/pull-requests/collaborating-with-pull-requests/working-with-forks/configuring-a-remote-for-a-fork) ayarlamak için aşağıdaki komutu kullanın:

`git remote add upstream https://github.com/MystenLabs/sui.git`

Artık yerel çalışma alanınızda Sui deposunun bir çatalı kurulmuştur. Çalışma alanındaki dosyalarda değişiklikler yapabilir, taahhütler ekleyebilir ve ardından Pull Request oluşturmak için değişikliklerinizi deponun çatalına gönderebilirsiniz.

### Daha fazla <a href="#further-reading" id="further-reading"></a>

* Halka açık sitemizde [Mysten Labs](https://mystenlabs.com/) şirketi hakkında bilgi edinin.
* [Sui Akıllı Kontrat Platformu](https://docs.sui.io/paper/sui.pdf) teknik dokümanını okuyun.
* Geliştirmenizin davranışını gözlemlemek için Sui'de [günlük kaydı ](https://docs.sui.io/devnet/contribute/observability)uygulamak.
* İlgili[ araştırma makalelerini](https://docs.sui.io/devnet/contribute/research-papers) bulun.
* [Davranış kurallarımızı](https://docs.sui.io/devnet/contribute/code-of-conduct) okuyun ve bunlara uyun.
