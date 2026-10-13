# AliExpress — WebView

Estado: o extrator local cobre JSON-LD, cards e campos complementares usando fixtures sintéticas. O teste mantém o preço como texto até o parser Rust e ignora produtos cujo preço esteja em moeda diferente de BRL; não há conversão.

As classes da página podem mudar. A captura renderizada real ainda é necessária para validar os seletores e o carregamento tardio de cards, conforme as Fases C2 e C3.

**Pendente:** capturar a página pelo modo diagnóstico e conferir os primeiros resultados e preços antes de declarar a loja aceita.
