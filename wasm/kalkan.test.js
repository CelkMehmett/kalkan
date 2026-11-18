// kalkan.js sarmalayıcısının testi. Tarayıcı/WASM yerine aynı C ABI'yi yerel
// kütüphane olarak bun:ffi ile yükler; sarmalayıcı kodu birebir aynıdır.
//
//   cargo build --release --features ffi
//   bun test wasm/
import { dlopen, FFIType as T, toArrayBuffer } from "bun:ffi";
import { fileURLToPath } from "node:url";
import { expect, test } from "bun:test";
import { KalkanHatasi, KalkanMotoru } from "./kalkan.js";

const lib = dlopen(fileURLToPath(new URL("../target/release/libkalkan.so", import.meta.url)), {
  kalkan_alloc: { args: [T.u64_fast], returns: T.ptr },
  kalkan_free: { args: [T.ptr, T.u64_fast], returns: T.void },
  kalkan_out_ptr: { args: [], returns: T.ptr },
  kalkan_out_len: { args: [], returns: T.u32 },
  kalkan_kinds_json: { args: [], returns: T.u32 },
  kalkan_session_new: { args: [T.u32, T.u32, T.u32, T.ptr, T.u64_fast], returns: T.u32 },
  kalkan_session_free: { args: [T.u32], returns: T.void },
  kalkan_mask: { args: [T.u32, T.ptr, T.u64_fast], returns: T.u32 },
  kalkan_restore: { args: [T.u32, T.ptr, T.u64_fast], returns: T.u32 },
  kalkan_detect_json: { args: [T.u32, T.ptr, T.u64_fast], returns: T.u32 },
  kalkan_audit_json: { args: [T.u32, T.ptr, T.u64_fast], returns: T.u32 },
  kalkan_vault_json: { args: [T.u32], returns: T.u32 },
  kalkan_stream_new: { args: [T.u32], returns: T.u32 },
  kalkan_stream_push: { args: [T.u32, T.ptr, T.u64_fast], returns: T.u32 },
  kalkan_stream_finish: { args: [T.u32], returns: T.u32 },
});

// WASM arka ucunun yerel eşdeğeri.
const motor = new KalkanMotoru({
  ex: lib.symbols,
  yaz(bytes) {
    const p = lib.symbols.kalkan_alloc(Math.max(bytes.length, 1));
    if (bytes.length) new Uint8Array(toArrayBuffer(p, 0, bytes.length)).set(bytes);
    return p;
  },
  oku(ptr, len) {
    return new Uint8Array(toArrayBuffer(ptr, 0, len)).slice();
  },
});

const BELGE = "Sayın Ayşe Kaya 🙂, TC 10000000146, IBAN TR33 0006 1005 1978 6457 8413 26, IP 192.168.1.10";

test("maskele ve geri yükle", () => {
  const o = motor.oturum();
  const m = o.maskele(BELGE);
  expect(m).toBe("Sayın [AD_SOYAD_1] 🙂, TC [TCKN_1], IBAN [IBAN_1], IP [IP_1]");
  expect(o.geriYukle(m)).toBe(BELGE);
  expect(Object.keys(o.kasa()).sort()).toEqual(["[AD_SOYAD_1]", "[IBAN_1]", "[IP_1]", "[TCKN_1]"]);
  o.kapat();
});

test("tespit konumları JS dizgi indeksi (UTF-16, emoji dahil)", () => {
  const o = motor.oturum();
  const ents = o.tespit(BELGE);
  expect(ents.map((e) => e.kind)).toEqual(["AD_SOYAD", "TCKN", "IBAN", "IP"]);
  for (const e of ents) expect(BELGE.slice(e.start, e.end)).toBe(e.text);
  o.kapat();
});

test("akış: her kesim noktasında aynı sonuç", () => {
  const o = motor.oturum({ strateji: "sentetik", anahtar: 2 ** 40 + 7 });
  const m = o.maskele(BELGE);
  expect(m).not.toContain("10000000146");
  const cevap = `Özet: ${m}`;
  const beklenen = o.geriYukle(cevap);
  expect(beklenen).toBe(`Özet: ${BELGE}`);
  for (let k = 0; k <= cevap.length; k++) {
    const a = o.akis();
    expect(a.ekle(cevap.slice(0, k)) + a.ekle(cevap.slice(k)) + a.bitir()).toBe(beklenen);
  }
  o.kapat();
});

test("terimler, denetim, türler", () => {
  const o = motor.oturum({ terimler: ["Proje Atlas"] });
  expect(o.maskele("PROJE ATLAS bütçesi")).toBe("[OZEL_1] bütçesi");
  const d = o.denetle("[OZEL_1] ve [TCKN_4], destek 0212 444 55 66");
  expect(d.clean).toBe(false);
  expect(d.unknown_placeholders).toEqual(["[TCKN_4]"]);
  expect(d.new_pii).toEqual([{ kind: "TELEFON", text: "0212 444 55 66" }]);
  expect(motor.turler()).toContain("DOGUM_TARIHI");
  expect(motor.turler()).toEqual(expect.arrayContaining(["ADRES", "SGK"]));
  o.kapat();
});

test("hatalar", () => {
  const o = motor.oturum();
  o.kapat();
  expect(() => o.maskele("x")).toThrow(KalkanHatasi);
  expect(() => motor.oturum({ strateji: "yok" })).toThrow("bilinmeyen strateji");
});
