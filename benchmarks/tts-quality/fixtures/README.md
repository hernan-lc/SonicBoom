# Authorized synthetic reference fixtures

`synthetic-hola.wav` was generated locally using eSpeak NG 1.52.0's Spanish
voice at 150 words/minute. It is a synthetic voice, not a human recording. Its
exact input transcript is:

> Hola. Esta grabación es una voz sintética para una prueba independiente.

SHA-256: `66f9b9fccd61ec0994b67c32c630529fd8430d7a3e00b29bd65facf0a283fbfc`.
It provides the reference-containing-`hola` case; it is deliberately distinct
from the requested target `test`. The second reference is the existing English
synthetic eSpeak fixture `reference-espeak-en.wav` (eSpeak NG 1.52.0 default
`en` voice). Its exact input transcript is:

> This is a synthetic reference voice for reproducible speech generation testing. The quick brown fox jumps over the lazy dog.

SHA-256: `d760d45c9c763860c28834228c30a3b69e8999d5a5c17c52ea05dc42c915b68e`.
That reference does not contain `hola`. Neither fixture establishes natural speaker similarity.
These diagnostic derivatives carry no human voice consent claim. eSpeak NG is
GPL-3.0-or-later; no eSpeak engine code or binaries are redistributed here.

`synthetic-sin-hola.wav` uses eSpeak NG's Spanish `es+f3` voice at 150 wpm.
Exact transcript: `La lluvia cae sobre la ciudad.`
SHA-256: `503f65df1fc870fd7f603749889830d9c964f20a6c51e256bbbee0bbc9c911be`.
The archived Qwen A/B run used these two Spanish references, so a cross-language
reference is not a confound in the reference-echo test.
