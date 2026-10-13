(() => {
  const run = () => {
    window.__garimpoStopStoreExtraction?.();
    let observer;
    let poller;
    let timeout;
    let pending = false;
    let previousCount = -1;
    let withoutGrowth = 0;
    let latestList;
    const maxItems = 50;
    const stableScrolls = 3;

    const stop = () => {
      observer?.disconnect();
      clearInterval(poller);
      clearTimeout(timeout);
    };
    window.__garimpoStopStoreExtraction = stop;
    window.__garimpoRunStoreExtraction = run;

    const extract = () => __GARIMPO_STORE_EXTRACTOR__;
    const itemCount = (result) => {
      const candidates = [
        result?.itens,
        result?.data?.itens,
        result?.data?.items,
        result?.data?.anuncios,
        result?.data?.searchResult?.items,
        result?.data?.itemSummaries,
        result?.data?.aliexpress_affiliate_product_query_response?.resp_result?.result?.products?.product,
        result?.data?.data?.productOfferV2?.nodes,
        result?.data,
      ];
      return candidates.find(Array.isArray)?.length ?? 0;
    };
    const send = (payload) => {
      stop();
      window.__TAURI__?.event?.emit(__GARIMPO_STORE_EVENT__, {
        ...payload,
        url: location.href,
      }).catch((error) => console.error('Falha ao enviar a extração da loja:', error));
    };
    const scroll = () => {
      const height = Math.max(document.body?.scrollHeight ?? 0, document.documentElement?.scrollHeight ?? 0);
      window.scrollTo(0, height);
    };
    const check = () => {
      if (pending) return;
      let result;
      try {
        result = extract();
      } catch {
        return;
      }

      if (result?.verification || result?.login_required) {
        pending = true;
        observer?.disconnect();
        clearInterval(poller);
        clearTimeout(timeout);
        send(result);
        return;
      }
      if (!result) return;
      if (result.error || result.kind === 'product') {
        send(result);
        return;
      }
      if (result.kind !== 'list') return;

      const count = itemCount(result);
      latestList = result;
      if (count === 0 || count >= maxItems) {
        send(result);
        return;
      }
      if (count > previousCount) {
        previousCount = count;
        withoutGrowth = 0;
      } else {
        withoutGrowth += 1;
      }
      if (withoutGrowth >= stableScrolls) {
        send(latestList);
        return;
      }
      scroll();
    };

    observer = new MutationObserver(check);
    observer.observe(document.documentElement, { childList: true, subtree: true, characterData: true });
    poller = setInterval(check, 500);
    timeout = setTimeout(() => {
      if (latestList) send(latestList);
      else send({ error: 'Não foi possível ler as ofertas desta loja.' });
    }, 15000);
    check();
  };

  window.__garimpoRunStoreExtraction = run;
  run();
})();
