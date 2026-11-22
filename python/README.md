# kalkan (Python)

Türkçe kişisel verileri (TC kimlik, VKN, IBAN, telefon, kart, ad-soyad, adres, SGK sicil no, IP,
doğum tarihi...) LLM'e gitmeden maskeleyen, cevapta geri yükleyen hızlı KVKK motoru. Rust ile
yazılmıştır; saf Python regex çözümlerine göre tek çekirdekte yaklaşık 8 kat hızlıdır ve
`mask_batch` GIL'i bırakarak tüm çekirdekleri kullanır.

## Kullanım

```bash
pip install kalkan
```

```python
import kalkan

k = kalkan.Kalkan(terms=["Proje Atlas"])
s = k.session()                       # her sohbet için bir oturum

istem = s.mask("Ayşe Kaya Hanım, TC 10000000146, Proje Atlas bütçesi")
# "[AD_SOYAD_1] Hanım, TC [TCKN_1], [OZEL_1] bütçesi"

cevap = llm(istem)
rapor = s.audit(cevap)                # model yeni kişisel veri üretti mi?
if not rapor:
    log.warning(rapor)
print(s.restore(cevap))

# Akış (streaming)
akis = s.stream()
for parca in llm_akisi(istem):
    gonder(akis.push(parca))
gonder(akis.finish())

# Toplu anonimleştirme: tüm çekirdeklerde, GIL serbest
temiz = kalkan.Kalkan(strategy="label").mask_batch(milyonlarca_kayit)
```

`detect()` sonuçlarındaki `start`/`end` karakter indeksidir: `metin[e.start:e.end]` doğrudan çalışır.
Kasa `s.vault` ile sözlük olarak alınır ve başka bir süreçte `import_vault` ile yüklenebilir.

## Entegrasyonlar

```python
from kalkan.messages import mask_messages                 # OpenAI SDK, bağımlılıksız
from kalkan.integrations.litellm import completion        # pip install "kalkan[litellm]"
from kalkan.integrations.litellm import KalkanGuardrail   # LiteLLM Proxy guardrail'i
from kalkan.integrations.langchain import with_kalkan     # pip install "kalkan[langchain]"
```

Hepsi akış cevaplarını da geri yükler; yer tutucu parçalar arasında bölünse bile.

LiteLLM Proxy'de (`config.yaml`):

```yaml
guardrails:
  - guardrail_name: kalkan
    litellm_params:
      guardrail: kalkan.integrations.litellm.KalkanGuardrail
      mode: [pre_call, post_call]
      default_on: true
```

LangChain'de:

```python
model = with_kalkan(ChatOpenAI(model="gpt-4o"))
zincir = prompt | model | StrOutputParser()
```

Tüm özellikler, sınırlamalar ve ölçümler için:
[github.com/CelkMehmett/kalkan](https://github.com/CelkMehmett/kalkan/blob/main/README.md).

> kalkan bir yardımcı araçtır, hukuki uyum garantisi değildir. Tespit %100 değildir.
