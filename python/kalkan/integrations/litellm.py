"""LiteLLM entegrasyonu: istemden önce maskele, cevapta (akış dahil) geri yükle.

SDK ile, ``litellm.completion`` yerine::

    from kalkan.integrations.litellm import completion

    cevap = completion(model="gpt-4o", messages=[{"role": "user", "content": "TC 10000000146"}])
    # Sağlayıcıya "TC [TCKN_1]" gider; cevap.choices[0].message.content gerçek değerleri içerir.

LiteLLM Proxy'de guardrail olarak (``config.yaml``)::

    guardrails:
      - guardrail_name: kalkan
        litellm_params:
          guardrail: kalkan.integrations.litellm.KalkanGuardrail
          mode: [pre_call, post_call]
          default_on: true

Her istek kendi oturumunu alır; kasa yalnızca süreç belleğinde tutulur, LiteLLM
loglarına veya metadata'ya **yazılmaz** (metadata'da yalnızca anlamsız bir kimlik durur).
"""
from __future__ import annotations

import copy
import uuid
from collections import OrderedDict
from typing import Any, AsyncIterator, Dict, Iterator, Optional, Sequence, Tuple

try:
    import litellm
    from litellm.integrations.custom_guardrail import CustomGuardrail
    from litellm.types.guardrails import GuardrailEventHooks
except ImportError as e:  # pragma: no cover
    raise ImportError('kalkan.integrations.litellm için LiteLLM gerekir: pip install "kalkan[litellm]"') from e

from .. import Kalkan, Session
from ..messages import map_json_text, mask_messages

__all__ = ["completion", "acompletion", "restore_response", "StreamFixer", "KalkanGuardrail"]

_DEFAULT: Optional[Kalkan] = None


def _default() -> Kalkan:
    global _DEFAULT
    if _DEFAULT is None:
        _DEFAULT = Kalkan()
    return _DEFAULT


def restore_response(response: Any, session: Session) -> Any:
    """``ModelResponse`` içindeki metin ve araç argümanlarını yerinde geri yükler."""
    for choice in getattr(response, "choices", None) or []:
        msg = getattr(choice, "message", None)
        if msg is None:
            continue
        if isinstance(getattr(msg, "content", None), str):
            msg.content = session.restore(msg.content)
        for call in getattr(msg, "tool_calls", None) or []:
            fn = getattr(call, "function", None)
            if fn is not None and isinstance(getattr(fn, "arguments", None), str):
                fn.arguments = map_json_text(fn.arguments, session.restore)
    return response


class StreamFixer:
    """Akış parçalarını (``ModelResponseStream``) yerinde geri yükler.

    Yer tutucu parçalar arasında bölünse bile (``"[TC"`` + ``"KN_1]"``) doğru çalışır.
    Seçim (``choice.index``) ve araç çağrısı başına ayrı bir geri yükleyici tutulur;
    kalan metin ``finish_reason`` taşıyan parçaya eklenir.
    """

    def __init__(self, session: Session) -> None:
        self._session = session
        self._streams: Dict[Tuple[int, Any], Any] = {}
        self._last_call: Dict[Tuple[int, Any], Any] = {}

    def _stream(self, key: Tuple[int, Any]) -> Any:
        s = self._streams.get(key)
        if s is None:
            s = self._streams[key] = self._session.stream()
        return s

    def fix(self, chunk: Any) -> Any:
        for choice in getattr(chunk, "choices", None) or []:
            idx = getattr(choice, "index", 0) or 0
            delta = getattr(choice, "delta", None)
            if delta is None:
                continue
            if isinstance(getattr(delta, "content", None), str) and delta.content:
                delta.content = self._stream((idx, "content")).push(delta.content)
            for call in getattr(delta, "tool_calls", None) or []:
                fn = getattr(call, "function", None)
                if fn is not None and isinstance(getattr(fn, "arguments", None), str) and fn.arguments:
                    key = (idx, ("tool", getattr(call, "index", 0)))
                    fn.arguments = self._stream(key).push(fn.arguments)
                    self._last_call[key] = call
            if getattr(choice, "finish_reason", None):
                self._flush(idx, delta)
        return chunk

    def _flush(self, idx: int, delta: Any) -> None:
        for key in [k for k in self._streams if k[0] == idx]:
            tail = self._streams.pop(key).finish()
            if not tail:
                continue
            if key[1] == "content":
                delta.content = (getattr(delta, "content", None) or "") + tail
            else:
                # Pratikte boştur (JSON argümanları "}" ile biter); yine de veri kaybolmasın.
                call = copy.deepcopy(self._last_call[key])
                call.function.arguments = tail
                delta.tool_calls = list(getattr(delta, "tool_calls", None) or []) + [call]


def _mask_args(args: tuple, kwargs: dict, kalkan: Optional[Kalkan]) -> Tuple[tuple, dict, Session]:
    session = (kalkan or _default()).session()
    if "messages" in kwargs:
        kwargs = {**kwargs, "messages": mask_messages(session, kwargs["messages"])}
    elif len(args) >= 2:
        args = (args[0], mask_messages(session, args[1]), *args[2:])
    return args, kwargs, session


def _sync_stream(it: Iterator[Any], fixer: StreamFixer) -> Iterator[Any]:
    for chunk in it:
        yield fixer.fix(chunk)


async def _async_stream(it: AsyncIterator[Any], fixer: StreamFixer) -> AsyncIterator[Any]:
    async for chunk in it:
        yield fixer.fix(chunk)


def completion(*args: Any, kalkan: Optional[Kalkan] = None, **kwargs: Any) -> Any:
    """``litellm.completion`` ile aynı imza; mesajlar maskelenir, cevap geri yüklenir.

    ``kalkan`` verilmezse varsayılan ayarlarla bir motor kullanılır. ``stream=True``
    ile dönen yineleyici geri yüklenmiş parçalar üretir.
    """
    args, kwargs, session = _mask_args(args, kwargs, kalkan)
    resp = litellm.completion(*args, **kwargs)
    if kwargs.get("stream"):
        return _sync_stream(resp, StreamFixer(session))
    return restore_response(resp, session)


async def acompletion(*args: Any, kalkan: Optional[Kalkan] = None, **kwargs: Any) -> Any:
    """``litellm.acompletion`` karşılığı."""
    args, kwargs, session = _mask_args(args, kwargs, kalkan)
    resp = await litellm.acompletion(*args, **kwargs)
    if kwargs.get("stream"):
        return _async_stream(resp, StreamFixer(session))
    return restore_response(resp, session)


class KalkanGuardrail(CustomGuardrail):
    """LiteLLM Proxy guardrail'i: ``pre_call``'da maskeler, ``post_call``'da geri yükler.

    ``config.yaml`` içindeki ``litellm_params`` alanları yapıcıya geçer:
    ``strategy``, ``terms``, ``kinds``, ``exclude``, ``min_confidence``, ``secret``.
    """

    #: Yanıtı gelmeyen (iptal edilmiş) istekler için bellekte tutulacak en fazla oturum.
    max_sessions = 10_000

    def __init__(
        self,
        strategy: str = "token",
        terms: Optional[Sequence[str]] = None,
        kinds: Optional[Sequence[str]] = None,
        exclude: Optional[Sequence[str]] = None,
        min_confidence: int = 70,
        secret: Optional[int] = None,
        **kwargs: Any,
    ) -> None:
        self.kalkan = Kalkan(
            strategy=strategy, terms=terms, kinds=kinds, exclude=exclude, min_confidence=min_confidence, secret=secret
        )
        self._sessions: "OrderedDict[str, Session]" = OrderedDict()
        kwargs.setdefault("guardrail_name", "kalkan")
        kwargs.setdefault("supported_event_hooks", [GuardrailEventHooks.pre_call, GuardrailEventHooks.post_call])
        kwargs.setdefault("event_hook", [GuardrailEventHooks.pre_call, GuardrailEventHooks.post_call])
        super().__init__(**kwargs)

    @staticmethod
    def _meta(data: dict) -> dict:
        for key in ("metadata", "litellm_metadata"):
            if isinstance(data.get(key), dict):
                return data[key]
        return data.setdefault("metadata", {})

    def _pop(self, data: Optional[dict]) -> Optional[Session]:
        if not isinstance(data, dict):
            return None
        sid = self._meta(data).get("kalkan_id")
        return self._sessions.pop(sid, None) if isinstance(sid, str) else None

    async def async_pre_call_hook(self, user_api_key_dict: Any, cache: Any, data: dict, call_type: Any) -> Any:
        messages = data.get("messages")
        if not isinstance(messages, list):
            return data
        session = self.kalkan.session()
        data["messages"] = mask_messages(session, messages)
        sid = uuid.uuid4().hex
        self._meta(data)["kalkan_id"] = sid
        self._sessions[sid] = session
        while len(self._sessions) > self.max_sessions:
            self._sessions.popitem(last=False)
        return data

    async def async_post_call_success_hook(self, data: dict, user_api_key_dict: Any, response: Any) -> Any:
        session = self._pop(data)
        if session is not None:
            restore_response(response, session)
        return response

    async def async_post_call_streaming_iterator_hook(
        self, user_api_key_dict: Any, response: Any, request_data: dict
    ) -> AsyncIterator[Any]:
        session = self._pop(request_data)
        fixer = StreamFixer(session) if session is not None else None
        async for chunk in response:
            yield fixer.fix(chunk) if fixer else chunk

    async def async_post_call_failure_hook(
        self,
        request_data: dict,
        original_exception: Exception,
        user_api_key_dict: Any,
        traceback_str: Optional[str] = None,
    ) -> Any:
        self._pop(request_data)  # kasa bellekte kalmasın
        return None
