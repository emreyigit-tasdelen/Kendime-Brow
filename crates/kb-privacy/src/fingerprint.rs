//! Fingerprinting protection (architecture §4.4, task 2.6, validation V6).
//!
//! A script runs in the page's main world before any page script (renderer
//! `on_context_created`) in every http(s) frame. Standard level:
//! - canvas / WebGL readback and audio data get small noise that is stable for one
//!   (session, site) pair and different across sites and sessions,
//! - WebGL vendor/renderer strings are generic,
//! - hardware hints are fixed (8 cores, 8 GB),
//! - Battery, Bluetooth, USB, Serial and HID APIs are removed.
//!
//! Honest limit (§4.4): JS-level protection can be detected by a determined script; the goal
//! is defeating common commercial fingerprinting libraries, not Tor-level anonymity.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Off,
    Standard,
    Strict,
}

impl Level {
    pub fn parse(s: &str) -> Self {
        match s {
            "off" => Self::Off,
            "strict" => Self::Strict,
            _ => Self::Standard,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Standard => "standard",
            Self::Strict => "strict",
        }
    }
}

/// 32-bit FNV-1a, used to derive a per-site seed from the session seed.
pub fn fnv1a(data: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for b in data {
        h ^= u32::from(*b);
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

/// Site key used for the seed: registrable-ish host (last two labels; good enough for noise
/// stability — exactness is not security relevant here).
pub fn site_key(host: &str) -> String {
    let labels: Vec<&str> = host.trim_end_matches('.').split('.').collect();
    if labels.len() <= 2 || host.parse::<std::net::IpAddr>().is_ok() {
        return host.to_string();
    }
    labels[labels.len() - 2..].join(".")
}

/// Per-(session, site) seed.
pub fn site_seed(session_seed: u64, host: &str) -> u32 {
    let mut data = session_seed.to_le_bytes().to_vec();
    data.extend_from_slice(site_key(host).as_bytes());
    fnv1a(&data) | 1
}

/// The injected script for `level` with the given seed. Empty for `Off`.
pub fn script(level: Level, seed: u32) -> String {
    if level == Level::Off {
        return String::new();
    }
    let strict = level == Level::Strict;
    SCRIPT
        .replace("__SEED__", &seed.to_string())
        .replace("__STRICT__", if strict { "true" } else { "false" })
}

const SCRIPT: &str = r#"(() => {
  'use strict';
  if (window.__kbFp) return;
  Object.defineProperty(window, '__kbFp', { value: true });
  const STRICT = __STRICT__;
  let s = __SEED__ >>> 0;
  const rnd = () => { s ^= s << 13; s >>>= 0; s ^= s >>> 17; s ^= s << 5; s >>>= 0; return s; };
  const nativeize = (fn, name) => {
    const src = `function ${name}() { [native code] }`;
    Object.defineProperty(fn, 'toString', { value: () => src });
    return fn;
  };
  const wrap = (proto, name, make) => {
    const orig = proto && proto[name];
    if (typeof orig !== 'function') return;
    Object.defineProperty(proto, name, { value: nativeize(make(orig), name), writable: true, configurable: true });
  };
  const getter = (obj, name, value) => {
    try { Object.defineProperty(obj, name, { get: nativeize(() => value, `get ${name}`), configurable: true }); } catch (e) {}
  };

  // ---- canvas: deterministic noise on readback (a few low bits of a few pixels)
  const noisePixels = (data) => {
    const seed0 = s; let n = Math.max(1, Math.min(16, data.length >> 12));
    for (let i = 0; i < n; i++) { const p = (rnd() % (data.length >> 2)) << 2; data[p] ^= 1; data[p + 1] ^= rnd() & 1; }
    s = seed0;
  };
  const noisyCopy = (canvas) => {
    try {
      const w = canvas.width, h = canvas.height;
      if (!w || !h) return canvas;
      const c = document.createElement('canvas'); c.width = w; c.height = h;
      const ctx = c.getContext('2d'); ctx.drawImage(canvas, 0, 0);
      const img = origGetImageData.call(ctx, 0, 0, w, h); noisePixels(img.data); ctx.putImageData(img, 0, 0);
      return c;
    } catch (e) { return canvas; }
  };
  const C2D = window.CanvasRenderingContext2D && CanvasRenderingContext2D.prototype;
  const origGetImageData = C2D && C2D.getImageData;
  wrap(C2D, 'getImageData', (orig) => function (...a) { const img = orig.apply(this, a); noisePixels(img.data); return img; });
  const HC = window.HTMLCanvasElement && HTMLCanvasElement.prototype;
  const origToDataURL = HC && HC.toDataURL, origToBlob = HC && HC.toBlob;
  wrap(HC, 'toDataURL', () => function (...a) { return origToDataURL.apply(noisyCopy(this), a); });
  wrap(HC, 'toBlob', () => function (...a) { return origToBlob.apply(noisyCopy(this), a); });

  // ---- WebGL: generic vendor/renderer, noisy readPixels
  for (const G of [window.WebGLRenderingContext, window.WebGL2RenderingContext]) {
    if (!G) continue;
    wrap(G.prototype, 'getParameter', (orig) => function (p) {
      if (p === 0x9245) return 'Google Inc.';                     // UNMASKED_VENDOR_WEBGL
      if (p === 0x9246) return 'ANGLE (Generic GPU)';            // UNMASKED_RENDERER_WEBGL
      return orig.call(this, p);
    });
    wrap(G.prototype, 'readPixels', (orig) => function (...a) {
      const r = orig.apply(this, a); const buf = a[6];
      if (buf && buf.length) noisePixels(buf);
      return r;
    });
  }

  // ---- audio: tiny deterministic noise
  const audioNoise = (arr) => { const seed0 = s; for (let i = 0; i < arr.length; i += 97) arr[i] += ((rnd() % 1000) - 500) * 1e-9; s = seed0; };
  if (window.AudioBuffer) wrap(AudioBuffer.prototype, 'getChannelData', (orig) => function (...a) { const d = orig.apply(this, a); audioNoise(d); return d; });
  if (window.AnalyserNode) wrap(AnalyserNode.prototype, 'getFloatFrequencyData', (orig) => function (arr) { orig.call(this, arr); audioNoise(arr); });

  // ---- hardware hints
  const NP = Navigator.prototype;
  getter(NP, 'hardwareConcurrency', 8);
  getter(NP, 'deviceMemory', 8);

  // ---- APIs with fingerprinting value and no use for most pages
  for (const k of ['getBattery']) { try { delete NP[k]; } catch (e) {} }
  for (const k of ['bluetooth', 'usb', 'serial', 'hid']) getter(NP, k, undefined);

  if (STRICT) {
    // Strict: fixed screen size buckets and UTC / en-US.
    const round = (v) => Math.ceil(v / 100) * 100;
    getter(Screen.prototype, 'width', round(screen.width));
    getter(Screen.prototype, 'height', round(screen.height));
    getter(Screen.prototype, 'availWidth', round(screen.width));
    getter(Screen.prototype, 'availHeight', round(screen.height));
    getter(NP, 'languages', ['en-US', 'en']);
    getter(NP, 'language', 'en-US');
    wrap(Date.prototype, 'getTimezoneOffset', () => function () { return 0; });
  }
})();"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels() {
        assert_eq!(Level::parse("strict"), Level::Strict);
        assert_eq!(Level::parse("whatever"), Level::Standard);
        assert_eq!(Level::parse(Level::Off.as_str()), Level::Off);
        assert!(script(Level::Off, 1).is_empty());
        assert!(script(Level::Strict, 7).contains("const STRICT = true"));
        assert!(script(Level::Standard, 7).contains("let s = 7 >>> 0"));
    }

    #[test]
    fn seeds_are_stable_per_site_and_differ_across_sites_and_sessions() {
        assert_eq!(site_key("www.news.co"), "news.co");
        assert_eq!(site_key("192.168.1.1"), "192.168.1.1");
        let a = site_seed(1, "www.news.org");
        assert_eq!(a, site_seed(1, "cdn.news.org"));
        assert_ne!(a, site_seed(1, "other.org"));
        assert_ne!(a, site_seed(2, "www.news.org"));
        assert_eq!(a & 1, 1, "seed must be non-zero for xorshift");
    }
}
