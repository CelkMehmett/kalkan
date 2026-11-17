// Demo sayfasının (demo.html) mantık testi: sayfa happy-dom'a yüklenir, WASM
// yerine aynı C ABI yerel kütüphaneden bun:ffi ile bağlanır.
//
//   cargo build --release --features ffi
//   bun add -d happy-dom && bun test wasm/
import { dlopen, FFIType as T, toArrayBuffer } from "bun:ffi";
import { expect, test } from "bun:test";
import { Window } from "happy-dom";
import { KalkanMotoru } from "./kalkan.js";

const ABI = Object.fromEntries(
  Object.entries({
    kalkan_alloc: [[T.u64_fast], T.ptr],
    kalkan_free: [[T.ptr, T.u64_fast], T.void],
    kalkan_out_ptr: [[], T.ptr],
    kalkan_out_len: [[], T.u32],
    kalkan_kinds_json: [[], T.u32],
    kalkan_session_new: [[T.u32, T.u32, T.u32, T.ptr, T.u64_fast], T.u32],
    kalkan_session_free: [[T.u32], T.void],
    kalkan_mask: [[T.u32, T.ptr, T.u64_fast], T.u32],
    kalkan_restore: [[T.u32, T.ptr, T.u64_fast], T.u32],
    kalkan_detect_json: [[T.u32, T.ptr, T.u64_fast], T.u32],
    kalkan_audit_json: [[T.u32, T.ptr, T.u64_fast], T.u32],
    kalkan_vault_json: [[T.u32], T.u32],
    kalkan_stream_new: [[T.u32], T.u32],
    kalkan_stream_push: [[T.u32, T.ptr, T.u64_fast], T.u32],
    kalkan_stream_finish: [[T.u32], T.u32],
  }).map(([k, [args, returns]]) => [k, { args, returns }]),
);
const lib = dlopen(new URL("../target/release/libkalkan.so", import.meta.url).pathname, ABI);
const motor = new KalkanMotoru({
  ex: lib.symbols,
  yaz(b) {
    const p = lib.symbols.kalkan_alloc(Math.max(b.length, 1));
    if (b.length) new Uint8Array(toArrayBuffer(p, 0, b.length)).set(b);
    return p;
  },
  oku: (p, n) => new Uint8Array(toArrayBuffer(p, 0, n)).slice(),
});

async function sayfaAc() {
  const html = await Bun.file(new URL("./demo.html", import.meta.url)).text();
  const window = new Window({
    settings: { disableJavaScriptEvaluation: true, disableCSSFileLoading: true, disableJavaScriptFileLoading: true },
  });
  window.document.write(html.replace(/<link[^>]+>/g, ""));
  const betik = html.match(/<script type="module">([\s\S]*?)<\/script>/)[1]
    .replace(/^import .*__KALKAN_JS__\*\/$/m, "")
    .replaceAll("import.meta.url", '"http://yerel/"');
  const AsyncFunction = Object.getPrototypeOf(async () => {}).constructor;
  await new AsyncFunction("kalkanYukle", "document", "atob", betik)(async () => motor, window.document, atob);
  return window.document;
}

const bekle = (ms = 120) => new Promise((r) => setTimeout(r, ms));

test("sayfa açılınca örnek metin maskelenir, kasa ve örnek cevap dolar", async () => {
  const d = await sayfaAc();
  expect(d.getElementById("hata").hidden).toBe(true);
  const cikti = d.getElementById("cikti");
  expect(cikti.textContent).toContain("[TCKN_1]");
  expect(cikti.textContent).toContain("[OZEL_1]"); // "Proje Atlas" terimi
  for (const g of ["10000000146", "Ayşe Kaya", "TR33", "192.168.1.10", "05.03.1990"]) {
    expect(cikti.textContent).not.toContain(g);
  }
  expect(cikti.querySelectorAll("mark").length).toBe(8);
  expect(d.getElementById("durum").textContent).toContain("8 kişisel veri bulundu");
  expect(d.getElementById("kasa").querySelectorAll("tr").length).toBe(8);
  // Örnek cevap geri yüklenir ve denetim temizdir.
  expect(d.getElementById("geri").textContent).toContain("Merhaba Ayşe Kaya");
  expect(d.getElementById("geri").textContent).toContain("TR33 0006 1005 1978 6457 8413 26");
  expect(d.getElementById("denetim").textContent).toContain("temiz");
});

test("kullanıcı yazınca ve strateji değişince güncellenir", async () => {
  const d = await sayfaAc();
  const g = d.getElementById("girdi");
  g.value = "Kartım 4111 1111 1111 1111";
  g.dispatchEvent(new d.defaultView.Event("input"));
  await bekle();
  expect(d.getElementById("cikti").textContent).toBe("Kartım [KART_1]");

  d.querySelector('[data-s="kismi"]').click();
  expect(d.getElementById("cikti").textContent).toBe("Kartım **** **** **** 1111");
  expect(d.querySelector('[data-s="kismi"]').getAttribute("aria-pressed")).toBe("true");
  expect(d.getElementById("kasa").textContent).toContain("geri yüklenemez");

  d.querySelector('[data-s="sentetik"]').click();
  const sahte = d.getElementById("cikti").textContent;
  expect(sahte).not.toContain("4111 1111 1111 1111");
  expect(sahte).toMatch(/^Kartım 4\d{3} \d{4} \d{4} \d{4}$/);
});

test("uydurma yer tutucu ve yeni kişisel veri denetimde uyarı verir", async () => {
  const d = await sayfaAc();
  const c = d.getElementById("cevap");
  c.value = "[TCKN_1] onaylandı. [IBAN_7] hesabına, destek 0212 444 55 66.";
  c.dispatchEvent(new d.defaultView.Event("input"));
  const kutu = d.getElementById("denetim");
  expect(kutu.classList.contains("uyari")).toBe(true);
  expect(kutu.textContent).toContain("[IBAN_7]");
  expect(kutu.textContent).toContain("0212 444 55 66");
  expect(d.getElementById("geri").textContent).toBe("10000000146 onaylandı. [IBAN_7] hesabına, destek 0212 444 55 66.");
  // Kullanıcı düzenlediği cevap, girdi değişince ezilmez.
  const g = d.getElementById("girdi");
  g.value += " ";
  g.dispatchEvent(new d.defaultView.Event("input"));
  await bekle();
  expect(c.value).toContain("[IBAN_7]");
});

test("HTML enjeksiyonuna karşı kaçış yapılır", async () => {
  const d = await sayfaAc();
  const g = d.getElementById("girdi");
  g.value = '<img src=x onerror=alert(1)> TC 10000000146';
  g.dispatchEvent(new d.defaultView.Event("input"));
  await bekle();
  expect(d.getElementById("cikti").querySelector("img")).toBeNull();
  expect(d.getElementById("cikti").textContent).toBe("<img src=x onerror=alert(1)> TC [TCKN_1]");
});
