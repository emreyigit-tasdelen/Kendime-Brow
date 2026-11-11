# İlerleme Notları — Kendime-Brow

Karar günlüğü ve oturumlar arası devir notu. En yeni giriş en üstte.
Yeni oturumda önce bu dosyayı ve [ROADMAP.md](ROADMAP.md) dosyasını oku.

## Şu anki durum (özet)

- **Faz:** 0 — mimari tasarım. Kod yazılmıyor.
- **Bekleyen karar:** Teknoloji temeli (Roadmap 0.4).
- **Sonraki adım:** Karar gelince `docs/mimari.md` yazılacak (Roadmap 0.5).

## Kalıcı kurallar

- Repo public. Anahtar, token, kişisel veri, Headscale kimlik bilgisi repoya girmez; her commit öncesi `git diff --cached` kontrol edilir.
- Commit kimliği GitHub noreply adresidir (kişisel e-posta public repoya yazılmaz).
- Lisans Apache 2.0. Eklenecek her kütüphanenin lisans uyumu mimari dokümanda not edilir (GPL/AGPL bileşenler ayrı süreçte izole edilir).
- Dokümanlar Türkçe; bir sonraki (daha ucuz) modelin devam edebileceği netlikte yazılır.

## Kararlar

| Tarih | Karar | Gerekçe |
|---|---|---|
| 2026-11-12 | Repo: `emreyigit-tasdelen/Kendime-Brow`, public, Apache 2.0 | Kullanıcı talebi |
| 2026-11-12 | VPN/Mesh: WireGuard + self-hosted Headscale | Kullanıcı talebi; üçüncü parti VPN'e bağımlılık yok |

## Günlük

### 2026-11-12
- Klasör, git, GitHub repo, LICENSE, .gitignore, README, ROADMAP, bu dosya oluşturuldu.
- Teknoloji temeli için seçenekler karşılaştırıldı ve kullanıcıya soruldu (aşağıda).

#### Teknoloji temeli seçenekleri (karar bekliyor)

| | A. Chromium fork | B. Electron | C. CEF + native kabuk | D. Firefox/Gecko fork | E. Tauri/sistem WebView |
|---|---|---|---|---|---|
| Web uyumu | Tam | Tam | Tam | İyi | Platforma göre değişir |
| Gizlilik kontrolü | En yüksek (kaynak seviyesinde yama) | Düşük (ağ yığınına sınırlı erişim) | Orta-yüksek | Yüksek | Düşük |
| RAM/CPU kontrolü | En yüksek | Düşük | Yüksek | Yüksek | Orta |
| Chrome extension desteği | Tam (MV2 dahil tutulabilir) | Kısmi | Kısmi | WebExtensions | Yok |
| Build maliyeti | Çok yüksek (100+ GB, saatler) | Düşük | Düşük (hazır binary) | Yüksek | Düşük |
| Güvenlik güncellemesi yükü | Yüksek (sürekli rebase) | Orta | Orta | Yüksek | Düşük |
| Tek geliştirici için uygunluk | Zor | Kolay ama tarayıcı için zayıf | Uygun | Zor | Tarayıcı için uygun değil |

**Öneri:** C (CEF + Rust native kabuk; modüller ayrı süreçlerde). Ayrıntı sohbette sunuldu.
