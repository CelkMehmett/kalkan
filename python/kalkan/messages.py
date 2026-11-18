"""OpenAI biçimli sohbet mesajlarını maskeleme / geri yükleme yardımcıları.

Bağımlılık gerektirmez; OpenAI SDK'sı, LiteLLM, Anthropic benzeri her API'nin
``messages`` listesiyle çalışır. Girdi değiştirilmez, yeni nesneler döner.

>>> import kalkan
>>> from kalkan.messages import mask_messages
>>> s = kalkan.Kalkan().session()
>>> mask_messages(s, [{"role": "user", "content": "TC 10000000146"}])
[{'role': 'user', 'content': 'TC [TCKN_1]'}]
>>> s.restore("[TCKN_1] doğrulandı")
'10000000146 doğrulandı'
"""
from __future__ import annotations

import json
from typing import Any, Callable, Dict, List, Mapping, Sequence

from ._kalkan import Session

__all__ = ["mask_messages", "restore_messages", "map_strings", "map_message", "map_json_text"]

# Mesaj içinde metin taşıyan, maskelenmesi gereken alanlar. Rol, ad, kimlik,
# model adı gibi alanlara dokunulmaz.
_TEXT_PART_KEYS = ("text", "input_text", "output_text")


def map_strings(value: Any, fn: Callable[[str], str]) -> Any:
    """Sözlük/liste içindeki tüm dizgilere ``fn`` uygular (araç argümanları için)."""
    if isinstance(value, str):
        return fn(value)
    if isinstance(value, Mapping):
        return {k: map_strings(v, fn) for k, v in value.items()}
    if isinstance(value, (list, tuple)):
        return type(value)(map_strings(v, fn) for v in value)
    return value


def map_json_text(text: str, fn: Callable[[str], str]) -> str:
    """JSON metnindeki (araç argümanları) dizgi değerlerine ``fn`` uygular.

    Değer JSON olarak ayrıştırılıp yeniden yazılır; böylece geri yüklenen değerde
    tırnak veya ters bölü olsa bile JSON bozulmaz. Geçerli JSON değilse (ör. akışta
    yarım argüman) düz metin olarak işlenir.
    """
    try:
        data = json.loads(text)
    except (ValueError, TypeError):
        return fn(text)
    return json.dumps(map_strings(data, fn), ensure_ascii=False)


def _map_content(content: Any, fn: Callable[[str], str]) -> Any:
    if isinstance(content, str):
        return fn(content)
    if isinstance(content, list):
        out = []
        for part in content:
            if isinstance(part, Mapping):
                part = dict(part)
                for key in _TEXT_PART_KEYS:
                    if isinstance(part.get(key), str):
                        part[key] = fn(part[key])
            elif isinstance(part, str):
                part = fn(part)
            out.append(part)
        return out
    return content


def map_message(message: Mapping[str, Any], fn: Callable[[str], str]) -> Dict[str, Any]:
    """Tek bir OpenAI biçimli mesajın metin alanlarına ``fn`` uygular.

    ``content`` (dizgi veya çok parçalı liste), ``tool_calls[].function.arguments``
    ve eski ``function_call.arguments`` alanları işlenir.
    """
    m = dict(message)
    if "content" in m:
        m["content"] = _map_content(m["content"], fn)
    if isinstance(m.get("tool_calls"), list):
        calls = []
        for call in m["tool_calls"]:
            call = dict(call)
            if isinstance(call.get("function"), Mapping):
                f = dict(call["function"])
                if isinstance(f.get("arguments"), str):
                    f["arguments"] = map_json_text(f["arguments"], fn)
                call["function"] = f
            calls.append(call)
        m["tool_calls"] = calls
    if isinstance(m.get("function_call"), Mapping):
        f = dict(m["function_call"])
        if isinstance(f.get("arguments"), str):
            f["arguments"] = map_json_text(f["arguments"], fn)
        m["function_call"] = f
    return m


def mask_messages(session: Session, messages: Sequence[Mapping[str, Any]]) -> List[Dict[str, Any]]:
    """Mesaj listesindeki kişisel verileri oturum kasasına alıp yer tutucuyla değiştirir.

    Aynı değer tüm mesajlarda aynı yer tutucuyu alır; cevabı aynı oturumla
    ``session.restore`` veya :func:`restore_messages` ile geri yükleyin.
    """
    return [map_message(m, session.mask) for m in messages]


def restore_messages(session: Session, messages: Sequence[Mapping[str, Any]]) -> List[Dict[str, Any]]:
    """:func:`mask_messages` ile maskelenmiş mesajlardaki yer tutucuları geri koyar."""
    return [map_message(m, session.restore) for m in messages]
