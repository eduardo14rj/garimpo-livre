# Testar o acesso sem controlar a área de trabalho

## Teste local reproduzível

Execute na raiz do projeto:

```powershell
npm run test:login
```

O teste executa o mesmo `login_navigation.js` injetado pelo app, com uma janela e um link simulados em Node. A fixture `login-diagnostico.json` contém somente o diagnóstico público enviado pelo Eduardo: endereço da página, link e texto do botão. Não contém cookies, credenciais ou dados de conta.

Cobre o clique em “Já tenho conta”, inclusive em um elemento filho do link, a navegação direta pelo mesmo WebView, preservação da query string e a recusa de links externos/desabilitados. O teste confirma que o script não depende de IPC injetado na página remota. Nenhuma página real é acessada.

Esses testes não usam captura de tela, teclado/mouse do Windows, perfil do navegador, banco de produção nem cofre do usuário. As operações se limitam aos arquivos do projeto e aos artefatos normais do runner de testes.

Para conferir a seleção de porta de desenvolvimento sem abrir uma janela Tauri:

```powershell
npm run test:dev-port
```

Se 1420 estiver ocupada, `npm run tauri dev` escolhe outra porta e um identificador Tauri de desenvolvimento próprio, separando os dados do perfil habitual.

## Validação real, limitada ao navegador

Para inspecionar uma página ao vivo, usar uma aba dedicada controlada pela ferramenta de navegador, com leitura de DOM, console e URL dessa aba. Isso dispensa acesso à área de trabalho inteira. Login e CAPTCHA continuam com o usuário.

Um teste local não comprova que o Mercado Livre aceitará a sessão ou concluirá o login real. A correção agora navega no WebView atual e não chama a ponte IPC da página externa. A validação final ainda exige observar a URL mudar na janela Tauri durante o clique real.

## Limite da evidência

O teste local comprova o tratamento do clique e o destino solicitado; não comprova que o Mercado Livre aceitará a sessão ou que a navegação real completará o login. Só considerar o erro ao vivo resolvido depois de observar a mudança de URL na janela Tauri.
