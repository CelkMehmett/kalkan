"""Entegrasyon testleri. LiteLLM/LangChain kurulu değilse ilgili testler atlanır.

Ağ kullanılmaz: LiteLLM için ``mock_response``, LangChain için sahte modeller.
"""
import asyncio
import json

import pytest

import kalkan
from kalkan.messages import mask_messages, restore_messages

TC = "10000000146"
ISTEM = f"Ben Ayşe Kaya Hanım, TC {TC}."
MASKELI = "Ben [AD_SOYAD_1] Hanım, TC [TCKN_1]."
CEVAP = "Merhaba [AD_SOYAD_1], [TCKN_1] kaydınız doğrulandı."
GERCEK = f"Merhaba Ayşe Kaya, {TC} kaydınız doğrulandı."


def run(coro):
    return asyncio.run(coro)


# --------------------------------------------------------------------- messages


def test_mask_messages_all_fields():
    s = kalkan.Kalkan(terms=['Acme "Holding"']).session()
    msgs = [
        {"role": "system", "content": "Yardımcı ol."},
        {"role": "user", "content": [{"type": "text", "text": ISTEM}, {"type": "image_url", "image_url": {"url": "x"}}]},
        {
            "role": "assistant",
            "content": None,
            "tool_calls": [{"id": "1", "type": "function", "function": {"name": "kayit", "arguments": json.dumps({"tc": TC, "firma": 'Acme "Holding"'})}}],
        },
    ]
    out = mask_messages(s, msgs)
    assert msgs[1]["content"][0]["text"] == ISTEM  # girdi değişmez
    assert out[0]["content"] == "Yardımcı ol."
    assert out[1]["content"][0]["text"] == MASKELI
    assert out[1]["content"][1] == msgs[1]["content"][1]
    assert json.loads(out[2]["tool_calls"][0]["function"]["arguments"]) == {"tc": "[TCKN_1]", "firma": "[OZEL_1]"}
    # Geri yüklenen değerde tırnak olsa bile JSON geçerli kalır.
    back = restore_messages(s, out)
    assert json.loads(back[2]["tool_calls"][0]["function"]["arguments"]) == {"tc": TC, "firma": 'Acme "Holding"'}
    assert back[1]["content"][0]["text"] == ISTEM


# ---------------------------------------------------------------------- LiteLLM


@pytest.fixture
def ll(monkeypatch):
    lm = pytest.importorskip("litellm")
    from kalkan.integrations import litellm as kl

    sent = []
    real, areal = lm.completion, lm.acompletion

    def fake(*a, **kw):
        sent.append(kw["messages"])
        return real(*a, **kw)

    async def afake(*a, **kw):
        sent.append(kw["messages"])
        return await areal(*a, **kw)

    monkeypatch.setattr(lm, "completion", fake)
    monkeypatch.setattr(lm, "acompletion", afake)
    return kl, sent, real, areal


def test_litellm_completion(ll):
    kl, sent, _, _ = ll
    r = kl.completion(model="gpt-4o", messages=[{"role": "user", "content": ISTEM}], mock_response=CEVAP)
    assert sent[0][0]["content"] == MASKELI
    assert r.choices[0].message.content == GERCEK


def test_litellm_stream_split_placeholders(ll):
    kl, sent, _, _ = ll
    # mock_response 3'er karakterlik parçalar üretir: "[TC" / "KN_" / "1]" gibi bölünür.
    parcalar = list(kl.completion(model="gpt-4o", messages=[{"role": "user", "content": ISTEM}], mock_response=CEVAP, stream=True))
    assert len(parcalar) > 5
    assert "".join(p.choices[0].delta.content or "" for p in parcalar) == GERCEK


def test_litellm_async_stream(ll):
    kl, _, _, _ = ll

    async def go():
        it = await kl.acompletion(model="gpt-4o", messages=[{"role": "user", "content": ISTEM}], mock_response=CEVAP, stream=True)
        return "".join([p.choices[0].delta.content or "" async for p in it])

    assert run(go()) == GERCEK


def test_litellm_guardrail_hooks(ll):
    kl, _, real, areal = ll
    from litellm.types.guardrails import GuardrailEventHooks

    g = kl.KalkanGuardrail(default_on=True)
    assert g.should_run_guardrail({}, GuardrailEventHooks.pre_call)
    assert g.should_run_guardrail({}, GuardrailEventHooks.post_call)

    data = {"model": "gpt-4o", "messages": [{"role": "user", "content": ISTEM}], "metadata": {}}
    data = run(g.async_pre_call_hook(None, None, data, "acompletion"))
    assert data["messages"][0]["content"] == MASKELI
    assert TC not in json.dumps(data["metadata"])  # kasa metadata'ya/loglara gitmez

    resp = real(model="gpt-4o", messages=data["messages"], mock_response=CEVAP)
    resp = run(g.async_post_call_success_hook(data, None, resp))
    assert resp.choices[0].message.content == GERCEK
    assert not g._sessions  # oturum serbest bırakıldı

    # Akış
    data = run(g.async_pre_call_hook(None, None, {"messages": [{"role": "user", "content": ISTEM}]}, "acompletion"))

    async def akis():
        src = await areal(model="gpt-4o", messages=data["messages"], mock_response=CEVAP, stream=True)
        return "".join([c.choices[0].delta.content or "" async for c in g.async_post_call_streaming_iterator_hook(None, src, data)])

    assert run(akis()) == GERCEK

    # Hata durumunda oturum bellekte kalmaz.
    data = run(g.async_pre_call_hook(None, None, {"messages": [{"role": "user", "content": ISTEM}]}, "acompletion"))
    run(g.async_post_call_failure_hook(data, RuntimeError("x"), None))
    assert not g._sessions


# -------------------------------------------------------------------- LangChain


@pytest.fixture
def lc():
    pytest.importorskip("langchain_core")
    from kalkan.integrations import langchain as klc

    return klc


def test_langchain_invoke_with_prompt(lc):
    from langchain_core.messages import AIMessage
    from langchain_core.output_parsers import StrOutputParser
    from langchain_core.prompts import ChatPromptTemplate
    from langchain_core.runnables import RunnableLambda

    seen = []

    def model(pv):
        seen.append(pv.to_messages()[-1].content)
        return AIMessage(CEVAP)

    prompt = ChatPromptTemplate.from_messages([("system", "Kısa cevap ver."), ("user", "{soru}")])
    chain = prompt | lc.with_kalkan(RunnableLambda(model)) | StrOutputParser()
    assert chain.invoke({"soru": ISTEM}) == GERCEK
    assert seen == [MASKELI]

    # Zincirin tamamı da sarılabilir: şablon değişkenleri maskelenir.
    seen.clear()
    assert lc.with_kalkan(prompt | RunnableLambda(model) | StrOutputParser()).invoke({"soru": ISTEM}) == GERCEK
    assert seen == [MASKELI]


def test_langchain_stream_split_and_tool_calls(lc):
    from langchain_core.messages import AIMessageChunk
    from langchain_core.runnables import RunnableGenerator

    seen = []

    def gen(inputs):
        for i in inputs:
            seen.append(i)
        for part in ["Merhaba [AD_SO", "YAD_1], [TC", "KN_1] kaydınız doğrulandı."]:
            yield AIMessageChunk(content=part)
        yield AIMessageChunk(content="", tool_call_chunks=[{"index": 0, "id": "c1", "name": "kayit", "args": '{"tc": "[TCK'}])
        yield AIMessageChunk(content="", tool_call_chunks=[{"index": 0, "id": None, "name": None, "args": 'N_1]"}'}])

    model = lc.with_kalkan(RunnableGenerator(gen))
    chunks = list(model.stream([("user", ISTEM)]))
    assert seen[0][0][1] == MASKELI
    total = chunks[0]
    for c in chunks[1:]:
        total = total + c
    assert total.content == GERCEK
    assert total.tool_calls[0]["args"] == {"tc": TC}


def test_langchain_async_and_on_leak(lc):
    from langchain_core.language_models.fake_chat_models import GenericFakeChatModel
    from langchain_core.messages import AIMessage

    raporlar = []
    fake = GenericFakeChatModel(messages=iter([AIMessage(CEVAP + " Destek: 0212 444 55 66"), AIMessage(CEVAP)]))
    model = lc.with_kalkan(fake, on_leak=raporlar.append)
    assert run(model.ainvoke(ISTEM)).content.startswith(GERCEK)
    assert raporlar and raporlar[0].new_pii == [("TELEFON", "0212 444 55 66")]

    async def akis():
        return "".join([c.content async for c in model.astream(ISTEM)])

    assert run(akis()) == GERCEK
    assert len(raporlar) == 1  # temiz cevap rapor üretmez
