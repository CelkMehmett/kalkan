"""LangChain entegrasyonu: herhangi bir Runnable'ı (sohbet modeli, zincir) sarar.

::

    from langchain_openai import ChatOpenAI
    from kalkan.integrations.langchain import with_kalkan

    model = with_kalkan(ChatOpenAI(model="gpt-4o"))
    model.invoke("Ben Ayşe Kaya, TC 10000000146. Kaydımı kontrol et.")
    # Modele "Ben [AD_SOYAD_1], TC [TCKN_1]. ..." gider; cevap gerçek değerlerle döner.

    zincir = prompt | with_kalkan(model) | StrOutputParser()   # LCEL
    for parca in zincir.stream({"soru": "..."}):                  # akış da geri yüklenir
        ...

Her ``invoke``/``stream`` çağrısı kendi oturumunu açar. Girdi olarak dizgi, mesaj
listesi, ``("user", "...")`` demetleri, ``PromptValue`` ve şablon değişkenleri
sözlüğü desteklenir; çıktıda dizgi, mesaj ve araç çağrısı argümanları geri yüklenir.
"""
from __future__ import annotations

from typing import Any, AsyncIterator, Callable, Dict, Iterator, List, Optional, Tuple

try:
    from langchain_core.messages import AIMessageChunk, BaseMessage
    from langchain_core.prompt_values import ChatPromptValue, StringPromptValue
    from langchain_core.runnables import Runnable, RunnableConfig
except ImportError as e:  # pragma: no cover
    raise ImportError('kalkan.integrations.langchain için langchain-core gerekir: pip install "kalkan[langchain]"') from e

from .. import Kalkan, LeakReport, Session
from ..messages import map_strings

__all__ = ["KalkanRunnable", "with_kalkan"]

Fn = Callable[[str], str]


def _map_content(content: Any, fn: Fn) -> Any:
    if isinstance(content, str):
        return fn(content)
    if isinstance(content, list):
        out = []
        for block in content:
            if isinstance(block, str):
                block = fn(block)
            elif isinstance(block, dict) and isinstance(block.get("text"), str):
                block = {**block, "text": fn(block["text"])}
            out.append(block)
        return out
    return content


def _map_message(m: BaseMessage, fn: Fn) -> BaseMessage:
    update: Dict[str, Any] = {"content": _map_content(m.content, fn)}
    tool_calls = getattr(m, "tool_calls", None)
    if tool_calls:
        update["tool_calls"] = [{**tc, "args": map_strings(tc.get("args", {}), fn)} for tc in tool_calls]
    return m.model_copy(update=update)


def _map(value: Any, fn: Fn) -> Any:
    """Girdi/çıktıdaki tüm metinlere ``fn`` uygular, türü korur."""
    if isinstance(value, str):
        return fn(value)
    if isinstance(value, BaseMessage):
        return _map_message(value, fn)
    if isinstance(value, ChatPromptValue):
        return ChatPromptValue(messages=[_map_message(m, fn) for m in value.messages])
    if isinstance(value, StringPromptValue):
        return StringPromptValue(text=fn(value.text))
    if isinstance(value, dict):
        if "role" in value and "content" in value:  # OpenAI biçimli mesaj sözlüğü
            return {**value, "content": _map_content(value["content"], fn)}
        return {k: _map(v, fn) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return type(value)(_map(v, fn) for v in value)
    return value


class _ChunkRestorer:
    """Akış parçalarını geri yükler; yer tutucu parçalar arasında bölünebilir."""

    def __init__(self, session: Session) -> None:
        self._session = session
        self._streams: Dict[Any, Any] = {}
        self._kind: Optional[str] = None
        self._tool_meta: Dict[Any, Dict[str, Any]] = {}

    def _push(self, key: Any, text: str) -> str:
        s = self._streams.get(key)
        if s is None:
            s = self._streams[key] = self._session.stream()
        return s.push(text)

    def fix(self, chunk: Any) -> Any:
        if isinstance(chunk, str):
            self._kind = "str"
            return self._push("content", chunk)
        if not isinstance(chunk, AIMessageChunk):
            return chunk
        self._kind = "msg"
        content = chunk.content
        if isinstance(content, str):
            content = self._push("content", content) if content else content
        elif isinstance(content, list):
            content = [
                {**b, "text": self._push(("block", b.get("index", 0)), b["text"])}
                if isinstance(b, dict) and isinstance(b.get("text"), str)
                else b
                for b in content
            ]
        tccs = []
        for tc in chunk.tool_call_chunks or []:
            key = ("tool", tc.get("index"))
            self._tool_meta[key] = {"index": tc.get("index"), "id": tc.get("id"), "name": None}
            args = tc.get("args")
            tccs.append({**tc, "args": self._push(key, args) if args else args})
        return _rebuild(chunk, content, tccs)

    def flush(self) -> List[Any]:
        """Akış bitince geri yükleyicilerde kalan metni son parça(lar) olarak döndürür."""
        tails: List[Tuple[Any, str]] = [(k, s.finish()) for k, s in self._streams.items()]
        self._streams.clear()
        tails = [(k, t) for k, t in tails if t]
        if not tails:
            return []
        if self._kind == "str":
            return ["".join(t for _, t in tails)]
        out = []
        for key, tail in tails:
            if key == "content":
                out.append(AIMessageChunk(content=tail))
            elif key[0] == "block":
                out.append(AIMessageChunk(content=[{"type": "text", "text": tail, "index": key[1]}]))
            else:
                meta = self._tool_meta.get(key, {"index": key[1], "id": None, "name": None})
                out.append(AIMessageChunk(content="", tool_call_chunks=[{**meta, "args": tail, "type": "tool_call_chunk"}]))
        return out


def _rebuild(chunk: AIMessageChunk, content: Any, tool_call_chunks: List[Dict[str, Any]]) -> AIMessageChunk:
    # tool_calls, tool_call_chunks'tan doğrulayıcıda türetilir: model_copy yerine yeniden kur.
    if not tool_call_chunks:
        return chunk.model_copy(update={"content": content})
    data = chunk.model_dump(exclude={"tool_calls", "invalid_tool_calls", "tool_call_chunks", "content"})
    return AIMessageChunk(content=content, tool_call_chunks=tool_call_chunks, **data)


class KalkanRunnable(Runnable):
    """Bir Runnable'ı saran maskele → çağır → geri yükle katmanı.

    ``on_leak`` verilirse model cevabı geri yüklenmeden önce ``session.audit`` ile
    denetlenir ve temiz değilse ``on_leak(rapor)`` çağrılır (akışta bitişte, tüm
    ham cevap üzerinden). İstisna fırlatarak cevabı engelleyebilirsiniz.
    """

    def __init__(
        self,
        inner: Runnable,
        kalkan: Optional[Kalkan] = None,
        on_leak: Optional[Callable[[LeakReport], None]] = None,
    ) -> None:
        self.inner = inner
        self.kalkan = kalkan or Kalkan()
        self.on_leak = on_leak

    @property
    def InputType(self) -> Any:  # noqa: N802 (LangChain adı)
        return self.inner.InputType

    @property
    def OutputType(self) -> Any:  # noqa: N802
        return self.inner.OutputType

    def _audit(self, session: Session, raw: Any, sep: str = "\n") -> None:
        if self.on_leak is None:
            return
        texts: List[str] = []
        _map(raw, lambda t: texts.append(t) or t)
        # Akışta parçalar ayırıcısız birleşir: bölünmüş bir numara da denetlensin.
        report = session.audit(sep.join(texts))
        if not report:
            self.on_leak(report)

    def invoke(self, input: Any, config: Optional[RunnableConfig] = None, **kwargs: Any) -> Any:
        session = self.kalkan.session()
        raw = self.inner.invoke(_map(input, session.mask), config, **kwargs)
        self._audit(session, raw)
        return _map(raw, session.restore)

    async def ainvoke(self, input: Any, config: Optional[RunnableConfig] = None, **kwargs: Any) -> Any:
        session = self.kalkan.session()
        raw = await self.inner.ainvoke(_map(input, session.mask), config, **kwargs)
        self._audit(session, raw)
        return _map(raw, session.restore)

    def stream(self, input: Any, config: Optional[RunnableConfig] = None, **kwargs: Any) -> Iterator[Any]:
        session = self.kalkan.session()
        fixer = _ChunkRestorer(session)
        raw: List[Any] = []
        for chunk in self.inner.stream(_map(input, session.mask), config, **kwargs):
            if self.on_leak is not None:
                raw.append(chunk)
            yield fixer.fix(chunk)
        self._audit(session, raw, sep="")
        yield from fixer.flush()

    async def astream(self, input: Any, config: Optional[RunnableConfig] = None, **kwargs: Any) -> AsyncIterator[Any]:
        session = self.kalkan.session()
        fixer = _ChunkRestorer(session)
        raw: List[Any] = []
        async for chunk in self.inner.astream(_map(input, session.mask), config, **kwargs):
            if self.on_leak is not None:
                raw.append(chunk)
            yield fixer.fix(chunk)
        self._audit(session, raw, sep="")
        for tail in fixer.flush():
            yield tail


def with_kalkan(
    runnable: Runnable,
    kalkan: Optional[Kalkan] = None,
    on_leak: Optional[Callable[[LeakReport], None]] = None,
) -> KalkanRunnable:
    """``runnable``'ı kişisel veri kalkanıyla sarar. Bkz. :class:`KalkanRunnable`."""
    return KalkanRunnable(runnable, kalkan=kalkan, on_leak=on_leak)
