---
description: Günlük, İzleme, Metrikler ve Gözlenebilirlik
---

# Logging (Günlük Kaydı)

İyi gözlemlenebilirlik özellikleri Sui'nin gelişimi ve büyümesi için kilit öneme sahiptir. Sui'nin dağıtık ve eşzamansız yapısı, potansiyel olarak küresel bir ağa dağıtılmış çoklu istemci ve validatör süreçleri ile bu durumu daha da zorlaştırmaktadır.

Sui'deki gözlemlenebilirlik yığını [Tokio izleme](https://tokio.rs/blog/2019-08-tracing) kütüphanesine dayanmaktadır. Bu belgenin geri kalanında, Sui'de yapılandırılmış günlük kaydı ve metrikler aracılığıyla iyi gözlemlenebilirlik elde etmenin belirli yönleri vurgulanmaktadır.

**Not:** Buradaki çıktı büyük ölçüde Sui operatörleri, yöneticileri ve geliştiricilerinin tüketimi içindir. Günlüklerin ve izlerin içeriği, validatörlerin yetkili, onaylı çıktılarını temsil etmez ve potansiyel olarak byzantine davranışa tabidir.

### Bağlamlar, kapsamlar ve işlem akışının izlenmesi <a href="#contexts-scopes-and-tracing-transaction-flow" id="contexts-scopes-and-tracing-transaction-flow"></a>

Sui gibi dağıtık ve eşzamansız bir sistemde, tek bir iş parçacığında zaman içinde bireysel günlüklere bakmaya güvenilemez. Bu sorunu çözmek için yapılandırılmış günlük kaydı yaklaşımını kullanın. Yapılandırılmış günlük kaydı, iş parçacıkları ve süreç sınırları boyunca günlükleri, olayları ve işlev bloklarını birbirine bağlamak için bir yol sunar.

#### Açıklıklar ve olaylar <a href="#spans-and-events" id="spans-and-events"></a>

[Tokio izleme](https://tokio.rs/blog/2019-08-tracing) kütüphanesinde, yapılandırılmış günlük kaydı [span'lar ve olaylar](https://docs.rs/tracing/0.1.31/tracing/index.html#core-concepts) kullanılarak uygulanır. Span'lar bir fonksiyon çağrısı, bir gelecek veya eşzamansız görev vb. gibi tüm bir işlevsellik bloğunu kapsar. İç içe yerleştirilebilirler ve aralıklardaki anahtar-değer çiftleri, işlev içindeki olaylara veya günlüklere bağlam sağlar.

* aralıkları ve bunların anahtar-değer çiftleri, kapalı günlüklere işlem kimliği gibi temel bir bağlam ekler.
* span'ler ayrıca kodun farklı bölümlerinde harcanan zamanı izleyerek dağıtılmış izleme işlevini etkinleştirir.
* Bireysel günlükler ayrıştırma, filtreleme ve toplamaya yardımcı olmak için anahtar-değer çiftleri de ekleyebilir.

Burada ilgi çekici bağlam bilgilerinin bir listesi bulunmaktadır:

* TX Digest'i
* Obje referans/ID, mümkünse
* Address
* Sertifika digest'i, mümkünse
* İstemci HTTP uç noktası için: rota, yöntem, durum
* Epoch (Dönem)
* Hem istemciler hem de validatörler için ana bilgisayar bilgileri

Hem bağlamı (tx digest) hem de gözlemlenebilirliği/filtrelemeyi geliştiren anahtar-değer çiftlerini gösteren ve bir işlemi ağ geçidi (`authority_aggregator`) ve validator boyunca izleyen örnek çıktı:

```
7ab7774d1f7bd40848}: sui_core::authority_aggregator: Broadcasting transaction request to authorities quorum_threshold=3 validity_threshold=2 timeout_after_quorum=60s
2022-03-05T01:35:03.383791Z TRACE test_move_call_args_linter_command:process_tx{tx_digest=t#7e5f08ab09ec80e3372c101c5858c96965a25326c21af27ab7774d1f7bd40848}: sui_core::authority_aggregator: Transaction data: TransactionData { kind: Call(MoveCall { package: (0000000000000000000000000000000000000002, SequenceNumber(1), o#3104eb8786a94f58d88564c38e22f13d79e3868c5cf81c9c9228fe91465eccad), module: Identifier("object_basics"), function: Identifier("transfer"), type_arguments: [], object_arguments: [(DA40C299F382CBC3C1EBEEA97351F5F185BAD359, SequenceNumber(1), o#d299113b3b52fd1b9dc01e3ba9cf70345faed592af04a56e287057f166ed2783)], shared_object_arguments: [], pure_arguments: [[145, 123, 205, 38, 175, 158, 193, 63, 122, 56, 238, 127, 139, 117, 186, 164, 89, 46, 222, 252]], gas_budget: 1000 }), sender: k#37ebb9c16574a57bcc7b52a6312a35991748be55, gas_payment: (3EE0283D2D12D5C49D0E4E2F509D07227A64ADF2, SequenceNumber(1), o#3ad1a71ee65e8e6675e6a0fb1e893e48c1820b274d3055d75f4abb850c9663e5) }
2022-03-05T01:35:03.385294Z DEBUG test_move_call_args_linter_command:process_tx{tx_digest=t#7e5f08ab09ec80e3372c101c5858c96965a25326c21af27ab7774d1f7bd40848}: sui_core::authority: Checked locks and found mutable objects num_mutable_objects=2
2022-03-05T01:35:03.386500Z DEBUG test_move_call_args_linter_command:process_tx{tx_digest=t#7e5f08ab09ec80e3372c101c5858c96965a25326c21af27ab7774d1f7bd40848}: sui_core::authority: Checked locks and found mutable objects num_mutable_objects=2
2022-03-05T01:35:03.387681Z DEBUG test_move_call_args_linter_command:process_tx{tx_digest=t#7e5f08ab09ec80e3372c101c5858c96965a25326c21af27ab7774d1f7bd40848}: sui_core::authority_aggregator: Received signatures response from authorities for transaction req broadcast num_errors=0 good_stake=3 bad_stake=0 num_signatures=3 has_certificate=true
2022-03-05T01:35:03.391891Z DEBUG test_move_call_args_linter_command:process_cert{tx_digest=t#7e5f08ab09ec80e3372c101c5858c96965a25326c21af27ab7774d1f7bd40848}: sui_core::authority_aggregator: Broadcasting certificate to authorities quorum_threshold=3 validity_threshold=2 timeout_after_quorum=60s
2022-03-05T01:35:03.394529Z DEBUG test_move_call_args_linter_command:process_cert{tx_digest=t#7e5f08ab09ec80e3372c101c5858c96965a25326c21af27ab7774d1f7bd40848}: sui_core::authority: Read inputs for transaction from DB num_inputs=3
2022-03-05T01:35:03.395917Z DEBUG test_move_call_args_linter_command:process_cert{tx_digest=t#7e5f08ab09ec80e3372c101c5858c96965a25326c21af27ab7774d1f7bd40848}: sui_core::authority: Finished execution of transaction with status Success { gas_used: 7 } gas_used=7
```

Yukarıdaki örnekte, `process_tx` ilk işlem isteğinin işlenmesini kapsayan bir açıklıktır ve "Checked locks" doğrulayıcıdaki işlem işleme yöntemi içindeki tek bir günlük mesajıdır.

Span içinde oluşan her günlük mesajı, `tx_digest` ve eklenen diğer alanlar da dahil olmak üzere span içinde tanımlanan anahtar-değer özelliklerini devralır. Günlük mesajları kendi anahtarlarını ve değerlerini ayarlayabilir. Günlüklerin span özelliklerini devralması, örneğin bir işlemin iş parçacığı ve işlem sınırları arasındaki akışını izlemenize olanak tanır.

### Anahtar-değer çiftleri şeması <a href="#key-value-pairs-schema" id="key-value-pairs-schema"></a>

Aralıklar tek bir olayı değil, tüm bir zaman bloğunu yakalar; böylece başlangıç, bitiş, süre vb. izleme, performans analizi vb. için yakalanabilir ve analiz edilebilir.

#### Açıklık isimleri <a href="#span-names" id="span-names"></a>

| İsim                       | Yer                  | İşlev                                                                                 |
| -------------------------- | -------------------- | ------------------------------------------------------------------------------------- |
| process\_tx                | Ağ Geçidi, Validatör | İşlem talebini gönderin, 2f+1 imzayı geri alın ve sertifika oluşturun                 |
| process\_cert              | Ağ Geçidi, Validatör | İşlemi yürütmek için validatörlere sertifika gönderme                                 |
| cert\_check\_signature     | Validatör            | Sertifika imzalarını kontrol edin                                                     |
| process\_cert\_inner       | Validatör            | Sertifikaları validatörde işlemek için iç fonksiyon                                   |
| fetch\_objects             | Validatör            | Veritabanından nesneleri okuma                                                        |
| tx\_execute\_to\_effects   | Validatör            | Move çağrısını yürütme ve efekt oluşturma                                             |
| tx\_execute                | Validatör            | Transfer / Move çağrısı vb. işlemlerin fiilen gerçekleştirilmesi                      |
| handle\_cert               | Ağ Geçidieway        | Sertifika işleme için bir validatöre gönderin                                         |
| quorum\_map\_auth          | Ağ Geçidi            | Bir ağ bileşenini tek bir validatörle işleme                                          |
| sync\_cert                 | Ağ Geçidi, Validatör | Validatör'e ağ geçidi tarafından başlatılan veri senkronizasyonu                      |
| db\_set\_transaction\_lock | Validatör            | Veritabanı yeni işlemde işlem kilitlerini ayarlar                                     |
| db\_update\_state          | Validatör            | Veritabanını sertifika ile güncelleyin, etkiler Move yürütme işlemi sonrasında başlar |
|                            |                      |                                                                                       |

#### Etiketler - anahtarlar <a href="#tags---keys" id="tags---keys"></a>

Buradaki fikir, her olay ve aralığın anahtar-değer çiftleriyle etiketlenmesidir. Herhangi bir bağlamda veya iç içe geçmiş bağlamlarda günlüğe kaydedilen olaylar da bağlam düzeyindeki etiketleri devralacaktır.

Bu etiketler analiz edilebilen ve filtrelenebilen _alanları_ temsil eder. Örneğin, yayınlar filtrelenebilir ve kötü hissenin belirli bir miktarı aştığı, ancak hata için yeterli olmadığı tüm örneklerin hataları görülebilir.

| Anahtar               | Mekan(lar)           | Anlamı                                                                         |
| --------------------- | -------------------- | ------------------------------------------------------------------------------ |
| tx\_digest            | Ağ Geçidi, Validatör | İşlemin onaltılı özeti                                                         |
| tx\_kind              | Ağ Geçidi, Validatör | İşlem türü: Aktar/Yayınla/Çağır                                                |
| quorum\_threshold     | Ağ Geçidi            | Bir işlem için gereken yeter sayı eşiği                                        |
| validity\_threshold   | Ağ Geçidi            | Tolere edilebilecek hatalardan kaynaklanan maksimum "kötü stake" sayısal eşiği |
| num\_errors           | Ağ Geçidi            | Validatörlerden gelen hata sayısı yayını                                       |
| good\_stake           | Ağ Geçidi            | Bir yayını yanıtlayan validatörlerden gelen toplam iyi stake miktarı           |
| bad\_stake            | Ağ Geçidi            | Hatalar da dahil olmak üzere validatörlerden gelen toplam hatalı stake miktarı |
| num\_signatures       | Ağ Geçidi            | Validatörlerden alınan imza sayısı yayını                                      |
| num\_unique\_effects  | Ağ Geçidi            | Validatörlerden gelen benzersiz efekt yanıtlarının sayısı                      |
| num\_inputs           | Validatör            | İşlem işleme için girdi sayısı                                                 |
| num\_mutable\_objects | Validatör            | İşlem işleme için değiştirilebilir nesne sayısı                                |
| gas\_used             | Validatör            | İşlem tarafından kullanılan gas miktarı                                        |
|                       |                      |                                                                                |

### Günlük seviyeleri <a href="#logging-levels" id="logging-levels"></a>

Bunun yüksek performanslı bir sistem olduğunu akılda tutarken, özellikle varsayılan olarak doğru miktarda ayrıntı dengelemek her zaman zordur.

| Seviye | Mesaj Tipleri                                                                                              |
| ------ | ---------------------------------------------------------------------------------------------------------- |
| Error  | Süreç düzeyinde hatalar (işlem düzeyinde hatalar değil, bunlardan bir sürü olabilir)                       |
| Warn   | Olağandışı veya Bizans tarzı faaliyetler                                                                   |
| Info   | Üst düzey toplu istatistikler, veri senkronizasyonu ile ilgili önemli olaylar, dönem değişiklikleri.       |
| Debug  | Münferit işlemler için üst düzey izleme, örneğin Ağ Geçidi/istemci tarafı -> validatör -> Move yürütme vb. |
| Trace  | Bireysel işlemler için son derece ayrıntılı izleme                                                         |
|        |                                                                                                            |

Bilgiden hata ayıklamaya geçmek çok daha büyük bir mesaj yığınıyla sonuçlanır.

`RUST_LOG` ortam değişkenini kullanarak hem genel günlük seviyesini hem de tek tek bileşenlerin seviyesini ayarlayabilirsiniz. Belirli aralıklara veya aralıklar içindeki etiketlere filtreleme yapmak bile mümkündür.

Daha fazla ayrıntı için [EnvFilter ](https://docs.rs/tracing-subscriber/latest/tracing\_subscriber/filter/struct.EnvFilter.html)konusuna bakın.

### Metrikler <a href="#metrics" id="metrics"></a>

Sui, Prometheus tabanlı ölçümler içerir:

* `rpc_requests_by_route` ve RPC Sunucu API ölçümleri ve gecikmeleri için ilgili (bkz. `rpc-server.rs`)
* Ağ geçidi işlem ölçümleri (`gateway-state.rs`'deki `GatewayMetrics` yapısına bakın)
* Validatör işlem metrikleri (bkz. `authority.rs`'deki `AuthorityMetrics`)

### Günlükleri, izleri, ölçümleri görüntüleme <a href="#viewing-logs-traces-metrics" id="viewing-logs-traces-metrics"></a>

İzleme mimarisi, çıktıyı işlemek ve görüntüleme için farklı lavabolara iletmek üzere izleme kütüphanesine takılabilen [aboneler ](https://github.com/tokio-rs/tracing#project-layout)fikrine dayanmaktadır. Aynı anda birden fazla abone aktif olabilir.

graph TB; Validator1 --> S1(open-telemetry) Validator1 --> S2(stdout logging) Validator1 --> S3(bunyan-formatter) S3 --> Vector Vector --> ElasticSearch S1 --> Jaeger Gateway --> SG1(open-telemetry) Gateway --> SG3(bunyan-formatter) SG1 --> Jaeger SG3 --> Vector2 Vector2 --> ElasticSearch

Yukarıdaki grafikte birden fazla abone bulunmaktadır. JSON günlüklerini, örneğin [Vector ](https://vector.dev/)gibi yerel bir yan günlük iletici aracılığıyla ve daha sonra ElasticSearch gibi hedeflere besleyebilirsiniz.

Vector gibi bir günlük ve metrik toplayıcının kullanılması, doğrulayıcı sunucuyu kesintiye uğratmadan kolay yeniden yapılandırmaya ve gözlemlenebilirlik trafiğini boşaltmaya olanak tanır.

Metrikler: varsayılan olarak `:9184/metrics` adresinde bir Prometheus kazıma uç noktası ile sunulur.

#### Stdout (varsayılan) <a href="#stdout-default" id="stdout-default"></a>

Varsayılan olarak, günlükler (ancak aralıklar değil) insan tarafından okunabilirlik için biçimlendirilir ve her satırın sonunda anahtar-değer etiketleri ile stdout'a çıktı verilir.

`RUST_LOG`'u filtreleme de dahil olmak üzere özel günlük çıktısı için yapılandırabilirsiniz - bu konunun başındaki [Günlük seviyeleri](https://docs.sui.io/devnet/contribute/observability#logging-levels) bölümüne bakın.

#### İzleme ve span çıkışı <a href="#tracing-and-span-output" id="tracing-and-span-output"></a>

Ayrıntılı span başlangıç ve bitiş günlükleri oluşturmak için `SUI_JSON_SPAN_LOGS` ortam değişkenini tanımlayın. Bu, tüm çıktının insan tarafından okunabilir olmayan JSON biçiminde olmasına neden olur, bu nedenle varsayılan olarak etkinleştirilmez.

Bu çıktıyı indeksleme, uyarılar, toplama ve analiz için bir araca veya hizmete gönderebilirsiniz.

Aşağıdaki örnek çıktı, yetkili kurumdaki sertifika işlemlerini açık günlük kaydı ile göstermektedir. `START` ve `END` ek açıklamalarına dikkat edin ve iç içe geçmiş `DB_UPDATE_STATE`'in `PROCESS_CERT` içine nasıl gömüldüğüne dikkat edin. Ayrıca her bir aralığın süresini günlüğe kaydeden `elapsed_milliseconds` değerine de dikkat edin.

```
{"v":0,"name":"sui","msg":"[PROCESS_CERT - START]","level":20,"hostname":"Evan-MLbook.lan","pid":51425,"time":"2022-03-08T22:48:11.241421Z","target":"sui_core::authority_server","line":67,"file":"sui_core/src/authority_server.rs","tx_digest":"t#d1385064287c2ad67e4019dd118d487a39ca91a40e0fd8e678dbc32e112a1493"}
{"v":0,"name":"sui","msg":"[PROCESS_CERT - EVENT] Read inputs for transaction from DB","level":20,"hostname":"Evan-MLbook.lan","pid":51425,"time":"2022-03-08T22:48:11.246688Z","target":"sui_core::authority","line":393,"file":"sui_core/src/authority.rs","num_inputs":2,"tx_digest":"t#d1385064287c2ad67e4019dd118d487a39ca91a40e0fd8e678dbc32e112a1493"}
{"v":0,"name":"sui","msg":"[PROCESS_CERT - EVENT] Finished execution of transaction with status Success { gas_used: 18 }","level":20,"hostname":"Evan-MLbook.lan","pid":51425,"time":"2022-03-08T22:48:11.246759Z","target":"sui_core::authority","line":409,"file":"sui_core/src/authority.rs","gas_used":18,"tx_digest":"t#d1385064287c2ad67e4019dd118d487a39ca91a40e0fd8e678dbc32e112a1493"}
{"v":0,"name":"sui","msg":"[DB_UPDATE_STATE - START]","level":20,"hostname":"Evan-MLbook.lan","pid":51425,"time":"2022-03-08T22:48:11.247888Z","target":"sui_core::authority","line":430,"file":"sui_core/src/authority.rs","tx_digest":"t#d1385064287c2ad67e4019dd118d487a39ca91a40e0fd8e678dbc32e112a1493"}
{"v":0,"name":"sui","msg":"[DB_UPDATE_STATE - END]","level":20,"hostname":"Evan-MLbook.lan","pid":51425,"time":"2022-03-08T22:48:11.248114Z","target":"sui_core::authority","line":430,"file":"sui_core/src/authority.rs","tx_digest":"t#d1385064287c2ad67e4019dd118d487a39ca91a40e0fd8e678dbc32e112a1493","elapsed_milliseconds":0}
{"v":0,"name":"sui","msg":"[PROCESS_CERT - END]","level":20,"hostname":"Evan-MLbook.lan","pid":51425,"time":"2022-03-08T22:48:11.248688Z","target":"sui_core::authority_server","line":67,"file":"sui_core/src/authority_server.rs","tx_digest":"t#d1385064287c2ad67e4019dd118d487a39ca91a40e0fd8e678dbc32e112a1493","elapsed_milliseconds":2}
```

#### Jaeger (dağıtılmış izleri görmek) <a href="#jaeger-seeing-distributed-traces" id="jaeger-seeing-distributed-traces"></a>

[Jaeger ](https://www.jaegertracing.io/)ile görselleştirilmiş iç içe açıklıkları görmek için aşağıdakileri yapın:

1.  Yerel bir Jaeger konteyneri almak için bunu çalıştırın:

    ```
    docker run -d -p6831:6831/udp -p6832:6832/udp -p16686:16686 jaegertracing/all-in-one:latest
    ```
2.  Sui'yi bu şekilde çalıştırın (trace en ayrıntılı aralıkları etkinleştirir):

    ```
    SUI_TRACING_ENABLE=1 RUST_LOG="info,sui_core=trace" ./sui start
    ```
3. Sui CLI client ile bazı transferleri çalıştırın veya benchmarking (performans testi) aracını çalıştırın.
4. `http://localhost:16686/` adresine gidin ve hizmet olarak Sui'yi seçin.

**Not:** Ayrı açıklıklar (iç içe olmayan) şimdilik tek bir iz olarak bağlanmamaktadır.

#### Canlı asenkron denetim / Tokio Console <a href="#live-async-inspection--tokio-console" id="live-async-inspection--tokio-console"></a>

[Tokio-console](https://github.com/tokio-rs/console), Tokio kullanarak Rust uygulamalarını gerçek zamanlı olarak analiz etmek ve hata ayıklamaya yardımcı olmak için tasarlanmış harika bir CLI aracıdır! Özel bir aboneye dayanır.

1. Sui'yi özel bir flag kullanarak oluşturun: `RUSTFLAGS="--cfg tokio_unstable" cargo build`.
2. Sui'yi `SUI_TOKIO_CONSOLE 1` olarak ayarlanmış şekilde başlatın.
3. Konsol deposunu klonlayın ve konsolu başlatmak için `cargo run` komutunu çalıştırın.

**Not:** Tokio-console desteğinin eklenmesi Sui validatörlerini/geçit yollarını önemli ölçüde yavaşlatabilir.

#### Bellek profili oluşturma <a href="#memory-profiling" id="memory-profiling"></a>

Sui, çoğu platformda varsayılan olarak [jemalloc bellek ayırıcısını](https://jemalloc.net/) kullanır ve jemalloc'un çok hafif olan ve üretim kullanımı için tasarlanmış örnekleme profilleyicisini kullanarak otomatik bellek profili oluşturmayı sağlayan kod vardır. Profil oluşturma kodu en fazla her 5 dakikada bir ve yalnızca toplam bellek varsayılan olarak %20 arttığında profil oluşturur. Profil oluşturma dosyaları `jeprof..MB.prof` olarak adlandırılır, böylece hata ayıklama kolaylığı için metrikler ve olaylarla ilişkilendirilmesi kolaydır.

Bellek profilinin çalışması için `_RJEM_MALLOC_CONF=prof:true` ortam değişkenini ayarlamanız gerekir. [Docker imajını](https://hub.docker.com/r/mysten/sui-node) kullanırsanız bunlar otomatik olarak ayarlanır.

[Bytehound ](https://github.com/koute/bytehound)gibi bazı ayırıcı tabanlı yığın profilleyicileri çalıştırmak, `jemalloc_ctl` istatistik API'lerine müdahale ettikleri veya uygulamadıkları için otomatik jemalloc profillemeyi devre dışı bırakacaktır.

Profil dosyalarını görüntülemek için, profillerin toplandığı platformda aşağıdakilerin yapılması gerekir:

1. `libunwind`'i, graphviz'in dot yardımcı programını ve jeprof'u yükleyin. Debian üzerinde: `apt-get install libjemalloc-dev libunwind-dev graphviz`.
2. Hata ayıklama sembolleriyle derleme: `cargo build --profile bench-profiling`
3. cd yapın:`$SUI_REPO/target/bench-profiling`
4. `jeprof --svg sui-node jeprof.xxyyzz.heap` komutunu çalıştırın - dosya adındaki zaman damgası ve bellek boyutuna göre yığın profilini seçin.

**Not:** Otomatik bellek profili oluşturma ile, daha önce listelenenlerin ötesinde ortam değişkenlerini yapılandırmak artık gerekli değildir. Özel profil oluşturma seçeneklerini yapılandırmak mümkündür:

* [Yığın (heap) Profili Oluşturma](https://github.com/jemalloc/jemalloc/wiki/Use-Case%3A-Heap-Profiling)
* [jemallocator ile Yığın (heap) profili oluşturma](https://gist.github.com/ordian/928dc2bd45022cddd547528f64db9174)

Örneğin, `_RJEM_MALLOC_CONF` öğesini şu şekilde ayarlayın: `prof:true,lg_prof_interval:24,lg_prof_sample:19`

Önceki ayar şu anlama gelir: profil oluşturmayı aç, ayrılan her 2^19 veya 512KB baytta bir örnekleme yap ve ayrılan her 2^24 veya 16MB bellekte bir profil dökümü al. Ancak, otomatik profil oluşturma daha iyi adlandırılmış ve daha az aralıklarla dosyalar üretmek için tasarlanmıştır, bu nedenle varsayılan yapılandırmanın geçersiz kılınması genellikle önerilmez.
