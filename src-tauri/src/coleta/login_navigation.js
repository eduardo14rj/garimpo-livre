(() => {
  // Captura somente o link de login na verificação do Mercado Livre.
  // A página remota não depende da ponte IPC do Tauri: navega no mesmo WebView,
  // mantendo o perfil e os cookies usados durante a coleta.
  window.addEventListener('click', (event) => {
    if (event.button !== 0 || event.ctrlKey || event.metaKey || event.shiftKey || event.altKey) return;
    if (location.pathname !== '/gz/account-verification') return;
    const link = event.composedPath().find((node) => node instanceof HTMLAnchorElement);
    if (!link || link.hasAttribute('download') || link.getAttribute('aria-disabled') === 'true'
        || link.hasAttribute('disabled') || link.closest('[inert]')) return;
    const url = new URL(link.href, location.href);
    const allowedHost = ['mercadolivre.com.br', 'mercadolivre.com', 'mercadolibre.com']
      .some((host) => url.hostname === host || url.hostname.endsWith(`.${host}`));
    if (url.protocol !== 'https:' || !allowedHost || url.username || url.password
        || url.pathname !== '/jms/mlb/lgz/login') return;

    // Evita que o handler hidratado da página cancele o link e navega direto
    // pela API do próprio WebView. on_navigation valida os domínios permitidos.
    event.preventDefault();
    event.stopImmediatePropagation();
    location.assign(url.href);
  }, true);
})();
