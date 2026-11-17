"""demo.html + kalkan.js + kalkan.wasm → tek dosyalık dist/index.html.

Ortaya çıkan dosya dışarıdan hiçbir şey yüklemeden çalışır;
GitHub Pages'e veya herhangi bir statik sunucuya olduğu gibi konabilir.
"""
import base64
import pathlib
import re

kok = pathlib.Path(__file__).parent
html = (kok / "demo.html").read_text(encoding="utf-8")
js = (kok / "kalkan.js").read_text(encoding="utf-8")
wasm = (kok / "kalkan.wasm").read_bytes()

js = re.sub(r"^export ", "", js, flags=re.M)
isaret = 'import { kalkanYukle } from "./kalkan.js"; /*__KALKAN_JS__*/'
assert isaret in html, "demo.html içinde JS işareti bulunamadı"
html = html.replace(isaret, js)
html = html.replace("__KALKAN_WASM_B64__", base64.b64encode(wasm).decode())

cikti = kok / "dist" / "index.html"
cikti.parent.mkdir(exist_ok=True)
cikti.write_text(html, encoding="utf-8")
print(f"{cikti} ({cikti.stat().st_size // 1024} KB)")
