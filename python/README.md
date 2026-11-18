# kalkan (Python)

A fast PII shield for Turkish text: masks personal data (national ID, IBAN, phone, card, names,
IP, date of birth, …) before it reaches an LLM and restores it in the reply. Built for KVKK
(Turkey's GDPR counterpart). Written in Rust; about 8× faster than pure-Python regex solutions on
a single core, and `mask_batch` uses all cores with the GIL released.

> **Türkçe:** Türkçe kişisel verileri LLM'e gitmeden maskeleyen, cevapta geri yükleyen KVKK
> motoru. Türkçe belgeler için [README.tr.md](https://github.com/CelkMehmett/kalkan/blob/main/README.tr.md).

## Usage

```bash
pip install kalkan
```

```python
import kalkan

k = kalkan.Kalkan(terms=["Proje Atlas"])
s = k.session()                        # one session per conversation

prompt = s.mask("Ayşe Kaya Hanım, TC 10000000146, Proje Atlas bütçesi")
# "[AD_SOYAD_1] Hanım, TC [TCKN_1], [OZEL_1] bütçesi"

reply = llm(prompt)
report = s.audit(reply)                # did the model produce new PII?
if not report:
    log.warning(report)
print(s.restore(reply))

# Streaming
stream = s.stream()
for chunk in llm_stream(prompt):
    send(stream.push(chunk))
send(stream.finish())

# Batch anonymization: all cores, GIL released
clean = kalkan.Kalkan(strategy="label").mask_batch(millions_of_records)
```

`start`/`end` in `detect()` results are character indices: `text[e.start:e.end]` works directly.
The vault is available as a dict via `s.vault` and can be loaded in another process with `import_vault`.

Full documentation, limitations and benchmarks:
[github.com/CelkMehmett/kalkan](https://github.com/CelkMehmett/kalkan).

> kalkan is a helper, not a legal compliance guarantee. Detection is not 100%.
