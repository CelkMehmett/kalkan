"""kalkan — Türkçe kişisel verileri LLM'e gitmeden maskeler, cevapta geri yükler.

>>> import kalkan
>>> k = kalkan.Kalkan()
>>> s = k.session()
>>> s.mask("Müşteri TC 10000000146, tel 0532 123 45 67")
'Müşteri TC [TCKN_1], tel [TELEFON_1]'
>>> s.restore("[TCKN_1] doğrulandı")
'10000000146 doğrulandı'
"""
from ._kalkan import (
    KINDS,
    Entity,
    Kalkan,
    LeakReport,
    Session,
    StreamRestorer,
    __version__,
    is_valid_iban,
    is_valid_tckn,
    is_valid_vkn,
    luhn_valid,
)

__all__ = [
    "KINDS", "Entity", "Kalkan", "LeakReport", "Session", "StreamRestorer", "__version__",
    "is_valid_iban", "is_valid_tckn", "is_valid_vkn", "luhn_valid",
]
