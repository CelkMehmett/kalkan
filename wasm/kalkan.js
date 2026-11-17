// kalkan.js — kalkan'ın WebAssembly sarmalayıcısı (bağımlılıksız ES modülü).
//
//   import { kalkanYukle } from "./kalkan.js";
//   const motor = await kalkanYukle("./kalkan.wasm");
//   const oturum = motor.oturum();
//   oturum.maskele("TC 10000000146");     // "TC [TCKN_1]"
//   oturum.geriYukle("[TCKN_1] tamam");   // "10000000146 tamam"
//
// Tüm işlem tarayıcıda yapılır; metin hiçbir sunucuya gitmez.

const enc = new TextEncoder();
const dec = new TextDecoder();
const STRATEJI = { token: 0, synthetic: 1, sentetik: 1, partial: 2, kismi: 2, label: 3, etiket: 3 };

/**
 * WASM modülünü yükler. `kaynak`: URL, Response, ArrayBuffer/Uint8Array veya WebAssembly.Module.
 * @returns {Promise<KalkanMotoru>}
 */
export async function kalkanYukle(kaynak) {
  let sonuc;
  if (kaynak instanceof WebAssembly.Module) {
    sonuc = { instance: await WebAssembly.instantiate(kaynak, {}) };
  } else {
    const r = typeof kaynak === "string" || kaynak instanceof URL ? await fetch(kaynak) : kaynak;
    const bytes = r instanceof Response ? await r.arrayBuffer() : r;
    sonuc = await WebAssembly.instantiate(bytes, {});
  }
  return new KalkanMotoru(wasmArkaUcu(sonuc.instance.exports));
}

/** WASM belleği üzerinden okuma/yazma. */
export function wasmArkaUcu(ex) {
  for (const ad of ["kalkan_alloc", "kalkan_free", "kalkan_out_ptr", "kalkan_out_len"]) {
    if (typeof ex[ad] !== "function") throw new KalkanHatasi(`eksik WASM dışa aktarımı: ${ad}`);
  }
  return {
    ex,
    yaz(bytes) {
      const p = ex.kalkan_alloc(bytes.length);
      new Uint8Array(ex.memory.buffer, p, bytes.length).set(bytes);
      return p;
    },
    oku(ptr, len) {
      return new Uint8Array(ex.memory.buffer, ptr, len).slice();
    },
  };
}

export class KalkanHatasi extends Error {}

export class KalkanMotoru {
  /** @param {{ex: object, yaz: Function, oku: Function}} arkaUc */
  constructor(arkaUc) {
    this.a = arkaUc;
    this.ex = arkaUc.ex;
  }

  /** Son çıktıyı okur; `n` hata kodu ise KalkanHatasi fırlatır. */
  _cikti(n) {
    const hata = n >>> 0 === 0xffffffff;
    const len = this.ex.kalkan_out_len() >>> 0;
    const s = len ? dec.decode(this.a.oku(this.ex.kalkan_out_ptr(), len)) : "";
    if (hata) throw new KalkanHatasi(s);
    return s;
  }

  /** Metni WASM belleğine yazar, `fn(ptr, len)` çağırır, belleği bırakır. */
  _metinle(metin, fn) {
    const bytes = enc.encode(metin);
    const p = this.a.yaz(bytes);
    try {
      return this._cikti(fn(p, bytes.length));
    } finally {
      this.ex.kalkan_free(p, bytes.length);
    }
  }

  /** Desteklenen türler: ["TCKN", "VKN", ...] */
  turler() {
    return JSON.parse(this._cikti(this.ex.kalkan_kinds_json()));
  }

  /**
   * Yeni oturum (kasa). Her sohbet için bir tane açın, işiniz bitince `kapat()`.
   * @param {{strateji?: string, anahtar?: number, terimler?: string[]}} [ayar]
   */
  oturum({ strateji = "token", anahtar = 0, terimler = [] } = {}) {
    const s = STRATEJI[strateji];
    if (s === undefined) throw new KalkanHatasi(`bilinmeyen strateji: ${strateji}`);
    const lo = anahtar >>> 0;
    const hi = Math.floor(anahtar / 2 ** 32) >>> 0;
    const bytes = enc.encode(terimler.join("\n"));
    const p = this.a.yaz(bytes);
    let h;
    try {
      h = this.ex.kalkan_session_new(s, lo, hi, p, bytes.length) >>> 0;
    } finally {
      this.ex.kalkan_free(p, bytes.length);
    }
    if (!h) throw new KalkanHatasi("oturum açılamadı");
    return new Oturum(this, h);
  }
}

export class Oturum {
  constructor(motor, h) {
    this.m = motor;
    this.h = h;
  }

  /** Metni maskeler. */
  maskele(metin) {
    return this.m._metinle(metin, (p, l) => this.m.ex.kalkan_mask(this.h, p, l));
  }

  /** LLM cevabındaki yer tutucuları gerçek değerlerle değiştirir. */
  geriYukle(metin) {
    return this.m._metinle(metin, (p, l) => this.m.ex.kalkan_restore(this.h, p, l));
  }

  /**
   * Kişisel verileri bulur. Konumlar JS dizgi indeksidir: `metin.slice(e.start, e.end)`.
   * @returns {{kind: string, start: number, end: number, confidence: number, text: string}[]}
   */
  tespit(metin) {
    return JSON.parse(this.m._metinle(metin, (p, l) => this.m.ex.kalkan_detect_json(this.h, p, l)));
  }

  /** Modelin ham cevabını denetler (geri yüklemeden önce). */
  denetle(hamCevap) {
    return JSON.parse(this.m._metinle(hamCevap, (p, l) => this.m.ex.kalkan_audit_json(this.h, p, l)));
  }

  /** Kasa: {"[TCKN_1]": "10000000146", ...} */
  kasa() {
    return JSON.parse(this.m._cikti(this.m.ex.kalkan_vault_json(this.h)));
  }

  /**
   * Akış (streaming) geri yükleyici. `ekle(parca)` hemen gösterilebilecek metni,
   * `bitir()` kalan kuyruğu döndürür.
   */
  akis() {
    const sh = this.m.ex.kalkan_stream_new(this.h) >>> 0;
    if (!sh) throw new KalkanHatasi("akış açılamadı");
    const m = this.m;
    // Parça bir emojinin (UTF-16 vekil çifti) ortasında bitebilir; yarım vekil
    // UTF-8'e çevrilemeyeceği için bir sonraki parçaya devredilir.
    let yarim = "";
    return {
      ekle: (parca) => {
        let s = yarim + parca;
        yarim = "";
        const son = s.charCodeAt(s.length - 1);
        if (son >= 0xd800 && son <= 0xdbff) {
          yarim = s.slice(-1);
          s = s.slice(0, -1);
        }
        return m._metinle(s, (p, l) => m.ex.kalkan_stream_push(sh, p, l));
      },
      bitir: () => {
        const kalan = yarim ? m._metinle(yarim, (p, l) => m.ex.kalkan_stream_push(sh, p, l)) : "";
        yarim = "";
        return kalan + m._cikti(m.ex.kalkan_stream_finish(sh));
      },
    };
  }

  /** Oturumu kapatır; kasadaki gerçek değerler bellekten silinir. */
  kapat() {
    this.m.ex.kalkan_session_free(this.h);
    this.h = 0;
  }
}
