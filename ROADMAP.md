# Roadmap — Kendime-Brow

Tüm işler buradan takip edilir. Tasarım ayrıntısı: [docs/mimari.md](docs/mimari.md)
(parantez içindeki numaralar o dokümanın bölümleridir).

**Kural:** Bir görev biter bitmez (ne kadar küçük olursa olsun) durumu burada güncelle ve
[ilerleme-notlari.md](ilerleme-notlari.md) → "Görev kayıtları" bölümüne giriş ekle, commit at.

**Durumlar:** `[ ]` bekliyor · `[~]` devam ediyor · `[x]` bitti · `[!]` engelli / karar bekliyor · `[-]` iptal

**Şu anki faz:** Faz 0 bitmek üzere — 0.6 (ek özellik seçimi) ve mimari onayı bekleniyor.

---

## Faz 0 — Kurulum ve mimari tasarım (kod yok)

- [x] 0.1 Çalışma klasörü, git, public GitHub repo
- [x] 0.2 LICENSE (Apache 2.0), .gitignore, README
- [x] 0.3 Roadmap ve ilerleme notları iskeleti
- [x] 0.4 Teknoloji temeli kararı → CEF + Rust
- [x] 0.5 `docs/mimari.md` — tam mimari tasarım dokümanı
- [!] 0.6 Ek özellik seçimi (mimari §12) — kullanıcı kararı bekleniyor
- [x] 0.7 Faz 1+ görevlerinin ayrıntılandırılması (bu dosya)
- [!] 0.8 Mimari onayı + açık sorular: hedef platform (K15), LICENSE'taki ad

## Faz 1 — Temel tarayıcı kabuğu

Çıktı: gezilebilen, sekmeli, kendi arayüzü olan pencere. Sürüm `0.1`.

- [ ] 1.1 **Doğrulama (spike):** V1, V3, V4 — CEF binary indir, Rust `cef` crate ile tek pencere aç, codec durumunu test et (§2, §14)
- [ ] 1.2 **Doğrulama:** V2 — codec'li kendi CEF build'i denemesi; sonuca göre karar kaydı
- [ ] 1.3 Workspace iskeleti: `crates/`, `ui/`, `proto/`, `scripts/`, `justfile`, `cef.lock` (§13)
- [ ] 1.4 CI: fmt, clippy, test, `cargo-deny` lisans denetimi, `gitleaks` (§13)
- [ ] 1.5 `kb://` özel şema + gömülü varlıklar + CSP (§3.4)
- [ ] 1.6 Pencere düzeni (CEF Views), sekme oluştur/kapat/değiştir, gezinme (§2.4)
- [ ] 1.7 `kb://ui`: sekme şeridi, adres çubuğu, geri/ileri/yenile (§3.4)
- [ ] 1.8 Arayüz ↔ kabuk köprüsü (message router, yalnız `kb://` kökenine açık) (§3.3)
- [ ] 1.9 `kb-ipc`: named pipe taşıma, protobuf, belirteçli el sıkışma (§3.3)
- [ ] 1.10 Modül konağı başlatıcı: AppContainer/kısıtlı token + Job Object, çökünce yeniden başlatma (§3.2)
- [ ] 1.11 Eylem kaydı + komut paleti (§3.5)
- [ ] 1.12 Profil, ayarlar (`kb://settings`), `kb.sqlite`: yer imleri, geçmiş, oturum geri yükleme (§3.7)
- [ ] 1.13 İndirme yöneticisi (temel) (§8.3)
- [ ] 1.14 Paketleme: Windows kurucu, tek görev çubuğu simgesi, `just install-dev` (§13)

## Faz 2 — Gizlilik ve performans çekirdeği

Çıktı: günlük kullanılabilir, Chrome'dan hafif ve reklamsız tarayıcı. Sürüm `0.2`.

- [ ] 2.1 Google servislerini kapatan bayrak/tercih seti + "sıfır istenmeyen bağlantı" CI testi (§4.1)
- [ ] 2.2 `kb-privacy`: adblock-rust entegrasyonu, istek engelleme (§4.2)
- [ ] 2.3 Filtre listesi yönetimi + güncelleme, kozmetik filtreleme, scriptlet'ler (§4.2)
- [ ] 2.4 İzleme parametresi temizleme, site başına kalkan düğmesi (§4.2)
- [ ] 2.5 Konteynerler (ayrı `CefRequestContext`), gizli pencere, üçüncü taraf çerez engeli (§4.3)
- [ ] 2.6 **Doğrulama:** V6 → parmak izi koruması enjeksiyonu, Standart/Sıkı seviyeler (§4.4)
- [ ] 2.7 HTTPS-Only, DoH, WebRTC politikası, ön yükleme kapatma, konteyner başına proxy (§4.5)
- [ ] 2.8 **Doğrulama:** V7 → sekme yaşam döngüsü: dondurma, atma, uyku, istisnalar (§5.2)
- [ ] 2.9 Bellek baskısı tepkisi, pil/ölçülü bağlantı modları (§5.2, §5.4)
- [ ] 2.10 Kaynak monitörü + `kb://tasks` (§5.5)
- [ ] 2.11 Performans ölçüm betikleri, bütçelerin CI'da izlenmesi (§5.1)
- [ ] 2.12 Otomatik güncelleyici (imzalı) (§13)

## Faz 3 — Dosya işleme modülleri

Her alt grup bağımsız yayınlanabilir. Sürüm `0.3`–`0.5`.

**3A — PDF (§6)**
- [ ] 3A.1 `kb-doc` konağı + PDFium render, `kb://pdf` görüntüleyici (döşeme, yakınlaştırma, arama)
- [ ] 3A.2 Birleştir / böl / sırala / döndür (lopdf)
- [ ] 3A.3 Form doldurma, açıklama ekleme
- [ ] 3A.4 qpdf: şifreleme, parola kaldırma, doğrusallaştırma, onarım
- [ ] 3A.5 Sıkıştırma: görsel yeniden örnekleme (DPI), renk profili (lcms2)
- [ ] 3A.6 OCR hattı (Tesseract) + dil paketi indirme
- [ ] 3A.7 Metin ve tablo çıkarma
- [ ] 3A.8 İmzalama (PAdES-B-B) + imza doğrulama
- [ ] 3A.9 Toplu işlem + eylem kaydına tüm PDF eylemleri

**3B — Ofis (§7)**
- [ ] 3B.1 `calamine` ile hızlı tablo önizleme (7a)
- [ ] 3B.2 Bileşen indirici (LibreOffice), `kb-doc-lok` alt süreci
- [ ] 3B.3 LOK döşeme render + `kb://office` görüntüleyici (7b)
- [ ] 3B.4 Format dönüştürme + toplu dönüştürme (7b)
- [ ] 3B.5 Yerinde düzenleme: girdi olayları, UNO komutlu araç çubuğu (7c)
- [ ] 3B.6 Stil/şablon yöneticisi, formül çubuğu, makro politikası (7d)

**3C — Medya ve dosyalar (§8)**
- [ ] 3C.1 `kb-media` konağı + FFmpeg bileşeni, `ffprobe` ile bilgi paneli
- [ ] 3C.2 Video: kırpma, dönüştürme, hazır ayarlar, ham argüman düzenleyici, kuyruk
- [ ] 3C.3 Fotoğraf: görüntüleyici, yıkıcı olmayan düzenleme, format dönüştürme
- [ ] 3C.4 **Doğrulama:** V11 → metadata/EXIF görüntüleme ve seçici silme
- [ ] 3C.5 RAW, AVIF, JXL, HEIC desteği; renk yönetimi
- [ ] 3C.6 `kb://files`: çift panelli gezgin, sürükle-bırak, arşiv (libarchive)
- [ ] 3C.7 İndirilenler: otomatik checksum, "şununla aç" eylemleri, kurallar

## Faz 4 — Uzak bağlantı ve ağ analizi

Sürüm `0.6`–`0.7`.

**4A — Kasa ve uzak bağlantı (§10)**
- [ ] 4A.1 `kb-vault`: kasa biçimi, keyring, ana parola, otomatik kilit (§10.2)
- [ ] 4A.2 Terminal altyapısı: xterm.js, `kb://term`, yerel kabuk (ConPTY)
- [ ] 4A.3 SSH (russh): parola + anahtar (kasa imzalama), `known_hosts`
- [ ] 4A.4 Bağlantı profilleri + içe aktarma (`ssh config`, PuTTY, `.rdp`) (§10.3)
- [ ] 4A.5 SFTP/SCP → `kb://files` uzak panel
- [ ] 4A.6 Tünelleme (yerel/uzak/dinamik) + konteyner proxy bağlama (§10.4)
- [ ] 4A.7 Telnet, FTP/FTPS
- [ ] 4A.8 **Doğrulama:** V8 → RDP (IronRDP), yerel alt pencere render, pano
- [ ] 4A.9 Çoklu oturum: bölünmüş panel, gruplar, broadcast, şifreli oturum logu (§10.5)

**4B — Ağ analizi (§9)**
- [ ] 4B.1 `kb-net` + CDP Network gözlemcisi, `kb://net` istek listesi ve ayrıntı
- [ ] 4B.2 NetLog entegrasyonu (DNS/TLS/soket olayları)
- [ ] 4B.3 Filtre dili (DSL) + alan kaydı
- [ ] 4B.4 HAR/cURL dışa aktarma, isteği düzenle–tekrar gönder
- [ ] 4B.5 `kb-capture`: Npcap/pktmon yakalama, tarayıcıya özel filtreleme
- [ ] 4B.6 TLS anahtar günlüğü + pcapng (DSB) yazma
- [ ] 4B.7 Protokol ayrıştırıcılar, akış birleştirme, üç bölmeli görünüm, istatistikler

## Faz 5 — VPN/Mesh, Companion, Headscale

Sürüm `0.8`. (§11)

- [ ] 5.1 `docs/sunucu-kurulum.md`: Headscale kurulum + sertleştirme + yedek/geri yükleme adımları; `deploy/headscale/*.example`
- [ ] 5.2 Sunucu kurulumu (kullanıcıyla birlikte; kimlik bilgileri repoya girmez)
- [ ] 5.3 **Doğrulama:** V10 → `kb-mesh` (Go/tsnet): Up/Down/Status/Dial, durum kasada
- [ ] 5.4 `kb://mesh`: kurulum sihirbazı, cihaz listesi, yol türü/gecikme
- [ ] 5.5 `kb-remote` ağ yolu = mesh; SOCKS5 → konteyner proxy
- [ ] 5.6 **Doğrulama:** V9 → `kb-companion`: tsnet düğümü, servis vekili, Windows servisi
- [ ] 5.7 Companion hazırlık denetçisi + durum API'si
- [ ] 5.8 Companion kurucu + kayıt akışı (ön anahtar)
- [ ] 5.9 Tek tıkla bağlan (RDP/SSH/SFTP) uçtan uca
- [ ] 5.10 ACL şablonu, Headscale yönetim paneli (isteğe bağlı)

## Faz 6 — Ek teknik kullanıcı araçları

_0.6 kararına göre kesinleşecek. Öneri sırası (mimari §12):_

- [ ] 6.1 P1: şifre yöneticisi (otomatik doldurma, KDBX), kullanıcı scriptleri, `kb://tools` (regex/JSON/hash)
- [ ] 6.2 P2: REST/API istemcisi, şifreli senkronizasyon, port tarayıcı/ping/traceroute, genel OCR, toplu yeniden adlandırma, ekran kaydı + GIF
- [ ] 6.3 **Doğrulama:** V5 → Chrome eklentisi (MV3) yükleme, en iyi çaba
- [ ] 6.4 Linux desteği (sandbox: seccomp + Landlock; paketleme)
