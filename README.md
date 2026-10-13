# Garimpo Livre

Aplicativo desktop em Tauri 2 para acompanhar buscas e comparar ofertas. O projeto usa extração determinística nas páginas abertas pelo WebView, lógica de ranking em Rust e persistência local em SQLite.

## Escopo atual

A correção v4 mantém a coleta somente por WebView. O app não usa APIs de lojas, credenciais, conversão de moeda nem estimativa de imposto. Preços que não estejam explícitos em reais ficam fora do ranking; frete desconhecido continua diferente de frete grátis.

O plano e o checklist estão em [docs/plano-v4.md](docs/plano-v4.md). Fixtures sintéticas e testes locais verificam as regras sem abrir o Tauri, visitar lojas ou usar contas. A aceitação com as lojas reais e as capturas de HTML renderizado continuam pendentes; o modo de diagnóstico está especificado na Fase C5.

## Testar sem abrir o aplicativo

```powershell
npm run test:extractor
npm run test:ui
npm run build:web
$env:CARGO_TARGET_DIR = (Join-Path $PWD 'src-tauri/target-codex-offline')
cargo test --manifest-path src-tauri/Cargo.toml
cargo clippy --manifest-path src-tauri/Cargo.toml -- -D warnings
cargo fmt --manifest-path src-tauri/Cargo.toml --all -- --check
```

`npm run test:multi` executa o pipeline com fixtures, SQLite temporário e UI em `jsdom`; ele não abre janelas nem usa a pasta de dados do aplicativo. Consulte [o escopo desses testes](docs/teste-multiloja-isolado.md).

## O que aprendi — Fase C4

Símbolos de variantes precisam ser normalizados antes da pontuação: `S25+`, `S25 +` e `S25plus` viram `s25 plus`. Sinônimos como `capa`, `capinha` e `case` permitem comparar títulos diferentes sem perder a intenção da busca. Quando o termo exige `Plus`, variantes `Ultra`, `Edge` ou `FE` sem `Plus` são filtradas.

Valores monetários e contagens são strings até serem interpretados por funções puras em Rust. Isso evita arredondamento intermediário e trata corretamente `R$ 1.234,56`, `4,5 de 5 estrelas` e `1.000+ vendidos`. O extrator AliExpress descarta um item sem preço em reais em vez de converter sua moeda.

## Iniciar no Windows

```powershell
npm install
npm run tauri dev
```

O comando `npm run dev` abre somente a interface no navegador. Para iniciar o aplicativo desktop, use `npm run tauri dev`. Se a porta Vite padrão estiver ocupada, o runner escolhe uma porta livre; não encerre outro processo do usuário para liberar a porta.

O desenvolvimento no Windows requer Rust com toolchain MSVC, Microsoft C++ Build Tools com **Desktop development with C++** e WebView2. Consulte os [pré-requisitos oficiais do Tauri](https://v2.tauri.app/start/prerequisites/).

## Gerar e instalar a versão de produção

Com Node.js 22.12 ou superior, Rust MSVC e as ferramentas C++ x64 instaladas, gere o instalador e a versão portable:

```powershell
npm install
npm run build
```

O build executa as verificações de TypeScript, extração e interface, os testes Rust e o Clippy. Depois empacota o app para Windows x64 e grava quatro arquivos em `release/<versão>/`: o instalador NSIS, o executável portable, `SHA256SUMS.txt` e `BUILD_INFO.txt`. Se o Visual Studio estiver instalado, o script carrega o ambiente C++ x64 automaticamente. `npm run build:web` gera somente o frontend; `npm run build:fast` gera a versão de produção sem executar os testes.

Execute `GarimpoLivre_<versão>_x64-setup.exe` para instalar no usuário atual, sem solicitar acesso de administrador. O instalador fica em português do Brasil e cria um atalho no Menu Iniciar. Se o Windows SmartScreen mostrar “O Windows protegeu o computador”, selecione **Mais informações → Executar assim mesmo**; esta versão não tem assinatura de código.

O instalador inclui o bootstrapper do WebView2. Windows 10/11 atualizado já inclui o runtime; se ele estiver ausente, a instalação do runtime precisa de conexão com a internet. O executável `GarimpoLivre_<versão>_x64-portable.exe` requer WebView2 instalado.

Buscas, histórico, sessões das lojas e perfil WebView ficam em `%APPDATA%\com.eduardo.garimpolivre\`. Atualizar instalando uma nova versão por cima preserva esses dados. Desinstalar não os remove. O executável portable usa a mesma pasta de dados.

## Coleta e limites

As lojas usam janelas WebView isoladas. Se uma página pedir login ou verificação, a pessoa resolve na janela visível; o app não tenta contornar esses controles. Os testes automáticos usam somente fixtures e conteúdo de DOM simulado.

As extrações em fixtures não comprovam que seletores ainda correspondem às páginas atuais. O checklist mantém a captura real e a aceitação manual abertas até haver um relatório de diagnóstico do WebView.
