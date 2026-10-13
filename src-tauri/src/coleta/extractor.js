(() => {
  window.__garimpoStopExtraction?.();

  const parsePrice = (value) => {
    if (typeof value === 'number') return Number.isFinite(value) ? value : null;
    const compact = String(value ?? '').replace(/R\$/g, '').replace(/\s/g, '');
    if (!compact) return null;
    const normalized = compact.includes(',') ? compact.replace(/\./g, '').replace(',', '.') : compact;
    const result = Number(normalized);
    return Number.isFinite(result) ? result : null;
  };
  const valueFrom = (node, selector) => node.querySelector(selector)?.textContent?.trim() ?? '';
  const moneyFrom = (node) => {
    if (!node) return null;
    const fraction = valueFrom(node, '.andes-money-amount__fraction');
    const cents = valueFrom(node, '.andes-money-amount__cents');
    return fraction ? parsePrice(`${fraction},${cents || '00'}`) : parsePrice(node.textContent);
  };
  const canonicalUrl = (url) => {
    try {
      const parsed = new URL(url, location.href);
      return `${parsed.host}${parsed.pathname}`.replace(/\/$/, '').toLowerCase();
    } catch { return ''; }
  };
  const idFromUrl = (url) => {
    try {
      const parsed = new URL(url, location.href);
      for (const [key, value] of parsed.searchParams) {
        const itemValue = /item[_-]?id[:=]([^&]+)/i.exec(value)?.[1] ?? value;
        if (/item[_-]?id/i.test(key) || /item[_-]?id[:=]/i.test(value)) {
          const itemId = itemValue.match(/MLBU?[-_]?\d+/i)?.[0];
          if (itemId) return itemId.replace(/[-_]/g, '').toUpperCase();
        }
      }
    } catch { /* use the ID from the URL path below */ }
    return String(url ?? '').match(/MLBU?[-_]?\d+/i)?.[0]?.replace(/[-_]/g, '').toUpperCase() ?? '';
  };
  const validString = (...values) => values.find((value) => typeof value === 'string' && value.trim())?.trim() ?? '';
  const validNumber = (...values) => {
    for (const value of values) {
      const parsed = parsePrice(value?.amount ?? value?.value ?? value);
      if (parsed !== null) return parsed;
    }
    return null;
  };
  const findProducts = (value, inheritedRating, visited = new Set()) => {
    if (!value || typeof value !== 'object' || visited.has(value)) return [];
    visited.add(value);
    if (Array.isArray(value)) return value.flatMap((item) => findProducts(item, inheritedRating, visited));
    const type = value['@type'];
    const isProduct = type === 'Product' || (Array.isArray(type) && type.includes('Product'));
    const rating = value.aggregateRating ?? inheritedRating;
    const found = isProduct ? [{ ...value, aggregateRating: value.aggregateRating ?? inheritedRating }] : [];
    return found.concat(Object.values(value).flatMap((item) => findProducts(item, rating, visited)));
  };

  const productOffer = (product) => Array.isArray(product?.offers) ? product.offers[0] : product?.offers;
  const jsonLdStrategy = () => {
    const products = [...document.querySelectorAll('script[type="application/ld+json"]')]
      .flatMap((script) => {
        try { return findProducts(JSON.parse(script.textContent ?? 'null', (_key,value,context) => typeof value === 'number' && context?.source ? context.source : value)); }
        catch { return []; }
      });
    return products.flatMap((product) => {
      const offer = productOffer(product);
      const url = validString(product.url, offer?.url);
      const id = validString(product.sku, product.productID, idFromUrl(url));
      const title = validString(product.name);
      if (!id && !url && !title) return [];
      const rating = product.aggregateRating;
      return [{
        id, titulo: title, preco: validNumber(offer?.price, offer?.lowPrice),
        preco_original: validNumber(offer?.highPrice), url,
        condicao: validString(offer?.itemCondition).split('/').pop().toLowerCase(),
        vendedor: validString(offer?.seller?.name), vendedor_id: null, vendedor_score: null,
        nota: validNumber(rating?.ratingValue),
        qtd_avaliacoes: validNumber(rating?.reviewCount, rating?.ratingCount), modelo: validString(product.model,product.mpn) || null,
        gtin:validString(product.gtin13,product.gtin14,product.gtin12,product.gtin8,product.gtin) || null,
        marca:validString(product.brand?.name,product.brand) || null,
        frete:typeof offer?.shippingDetails?.shippingRate?.value === 'string' && offer.shippingDetails.shippingRate.currency === 'BRL' ? offer.shippingDetails.shippingRate.value : null,
      }];
    });
  };

  const stateStrategy = () => {
    const roots = [window.__PRELOADED_STATE__, window.__INITIAL_STATE__, window.__NEXT_DATA__]
      .filter((value) => value && typeof value === 'object');
    const records = [];
    const visited = new Set();
    const walk = (value) => {
      if (!value || typeof value !== 'object' || visited.has(value)) return;
      visited.add(value);
      if (Array.isArray(value)) { value.forEach(walk); return; }
      const url = validString(value.permalink, value.url, value.href, value.link);
      const id = validString(value.id, value.item_id, value.itemId, value.sku, idFromUrl(url));
      const title = validString(value.title, value.name, value.product_title);
      const price = validNumber(value.price, value.current_price, value.sale_price, value.amount);
      if ((id || url) && title && (price !== null || value.condition || value.reviews)) {
        const seller = value.seller ?? value.seller_data ?? {};
        const reviews = value.reviews ?? value.rating ?? {};
        records.push({
          id, titulo: title, preco: price,
          preco_original: validNumber(value.original_price, value.originalPrice, value.list_price),
          url, condicao: validString(value.condition, value.condition_id),
          vendedor: validString(seller.nickname, seller.name, value.seller_name),
          vendedor_id: validString(seller.id, value.seller_id) || null,
          vendedor_score: validNumber(seller.seller_score, value.seller_score),
          vendedor_nivel: validString(seller.seller_reputation?.level_id, value.seller_reputation?.level_id),
          nota: validNumber(reviews.rating_average, reviews.ratingValue, value.rating_average),
          qtd_avaliacoes: validNumber(reviews.total, reviews.reviewCount, value.review_count) ?? 0,
          modelo: validString(value.model, value.model_name) || null,
          gtin:validString(value.gtin,value.ean) || null,marca:validString(value.brand?.name,value.brand) || null,
        });
      }
      Object.values(value).forEach(walk);
    };
    roots.forEach(walk);
    return records;
  };

  const domStrategy = () => {
    let cards = [...document.querySelectorAll('.poly-card')];
    if (cards.length === 0) cards = [...document.querySelectorAll('li.ui-search-layout__item')];
    return cards.flatMap((card, index) => {
      const anchor = card.querySelector('a.poly-component__title, a.ui-search-link, h2 a, h3 a, a[href]');
      const url = anchor?.href ?? '';
      const title = anchor?.textContent?.trim() ?? '';
      const price = parsePrice(card.dataset.price ?? card.querySelector('[itemprop="price"]')?.content)
        ?? moneyFrom(card.querySelector('.poly-price__current .andes-money-amount, .poly-price, .ui-search-price__second-line .andes-money-amount'));
      if (!title || !url || price === null) return [];
      const rating = Number(card.dataset.rating ?? card.querySelector('[itemprop="ratingValue"]')?.content
        ?? valueFrom(card, '.poly-reviews__rating, .poly-component__review-compacted .polylabel-label'));
      const reviewCount = Number(card.dataset.reviewCount ?? card.querySelector('[itemprop="reviewCount"]')?.content
        ?? valueFrom(card, '.poly-reviews__total, .ui-search-reviews__amount').replace(/[^0-9]/g, ''));
      return [{
        id: card.dataset.id ?? idFromUrl(url) ?? `item-${index + 1}`,
        titulo: title, preco: price,
        preco_original: parsePrice(card.dataset.originalPrice) ?? moneyFrom(card.querySelector('.andes-money-amount--previous')),
        url, condicao: card.dataset.condition ?? '',
        vendedor: valueFrom(card, '.poly-component__seller, .poly-seller, .ui-search-official-store-label'),
        vendedor_id: card.dataset.sellerId ?? null,
        vendedor_nivel: card.dataset.sellerLevel ?? null,
        vendedor_score: Number.isFinite(Number(card.dataset.sellerScore)) ? Number(card.dataset.sellerScore) : null,
        nota: Number.isFinite(rating) && rating > 0 ? rating : null,
        qtd_avaliacoes: Number.isFinite(reviewCount) ? reviewCount : 0,
        modelo: card.dataset.model ?? null,
        gtin:card.dataset.gtin ?? null,marca:card.dataset.brand ?? null,frete:card.dataset.shippingPrice ?? null,
      }];
    });
  };

  const mergeRecords = (sources) => {
    const merged = new Map();
    const fields = ['id', 'titulo', 'preco', 'preco_original', 'url', 'condicao', 'vendedor', 'vendedor_id', 'vendedor_score', 'vendedor_nivel', 'nota', 'qtd_avaliacoes', 'modelo','gtin','marca','frete'];
    sources.forEach(([strategy, records]) => records.forEach((record) => {
      const normalizedTitle = String(record.titulo ?? '').toLocaleLowerCase('pt-BR').replace(/\s+/g, ' ').trim();
      const normalizedId = String(record.id ?? '').toUpperCase();
      const normalizedUrl = canonicalUrl(record.url);
      let current = [...merged.values()].find((item) =>
        (normalizedId && item.record.id?.toUpperCase() === normalizedId)
        || (normalizedUrl && item.urls.has(normalizedUrl))
        || (normalizedTitle && record.preco != null && !item.sources.has(strategy)
          && item.record.titulo?.toLocaleLowerCase('pt-BR').replace(/\s+/g, ' ').trim() === normalizedTitle
          && item.record.preco === record.preco)
      );
      if (!current) {
        current = { record: {}, strategies: {}, urls: new Set(), sources: new Set() };
        merged.set(Symbol(), current);
      }
      current.sources.add(strategy);
      if (normalizedUrl) current.urls.add(normalizedUrl);
      fields.forEach((field) => {
        const value = record[field];
        const missing = value === null || value === undefined || value === '' || (field === 'preco' && value === 0);
        const hasValue = current.record[field] !== null && current.record[field] !== undefined && current.record[field] !== '';
        if (!missing && (!hasValue || (field === 'url' && strategy === 'dom'))) {
          current.record[field] = value;
          current.strategies[field] = strategy;
        }
      });
    }));
    return [...merged.values()].map(({ record, strategies }) => ({
      ...record,
      id: record.id || idFromUrl(record.url) || `item-${Math.random().toString(36).slice(2)}`,
      preco: record.preco ?? 0,
      preco_original: record.preco_original ?? null,
      condicao: record.condicao || 'new',
      vendedor: record.vendedor || '',
      vendedor_id: record.vendedor_id ?? null,
      vendedor_score: record.vendedor_score ?? 0.5,
      nota: record.nota ?? null,
      qtd_avaliacoes: record.qtd_avaliacoes ?? 0,
      modelo: record.modelo ?? null,
      estrategias: strategies,
    }));
  };

  const detailFromDom = () => {
    const title = document.querySelector('h1')?.textContent?.trim() ?? '';
    const priceNode = document.querySelector('.ui-pdp-price__second-line .andes-money-amount, .ui-pdp-price__main-container .andes-money-amount, [itemprop="price"]');
    const price = parsePrice(priceNode?.getAttribute('content')) ?? moneyFrom(priceNode);
    if (!title && price === null) return null;
    const rating = Number(document.querySelector('[itemprop="ratingValue"]')?.getAttribute('content') ?? valueFrom(document, '.ui-pdp-reviews__rating, .ui-review-capability__rating'));
    const reviews = Number(document.querySelector('[itemprop="reviewCount"]')?.getAttribute('content') ?? valueFrom(document, '.ui-pdp-reviews__amount').replace(/[^0-9]/g, ''));
    return {
      id: idFromUrl(location.href) || location.pathname.split('/').filter(Boolean).at(-1) || 'produto',
      titulo: title, preco: price, nota: Number.isFinite(rating) && rating > 0 ? rating : null,
      qtd_avaliacoes: Number.isFinite(reviews) ? reviews : 0, url: location.href,
    };
  };

  const extract = () => {
    const strategies = [
      ['jsonld', jsonLdStrategy()],
      ['estado', stateStrategy()],
      ['dom', domStrategy()],
    ];
    const anuncios = mergeRecords(strategies);
    const listPage = location.hostname === 'lista.mercadolivre.com.br';
    const schemaProducts = strategies[0][1];
    const samePage = (record) => canonicalUrl(record.url) === canonicalUrl(location.href);
    const selected = schemaProducts.find(samePage)
      ?? strategies[1][1].find(samePage)
      ?? (schemaProducts.length === 1 && !listPage ? schemaProducts[0] : null);
    const domProduct = !listPage ? detailFromDom() : null;
    const product = selected || (domProduct?.preco > 0 ? domProduct : null)
      ? mergeRecords([
        ['jsonld', selected ? [selected] : []],
        ['estado', strategies[1][1].filter(samePage)],
        ['dom', domProduct ? [domProduct] : []],
      ])[0]
      : null;
    const bodyText = (document.body?.innerText || document.body?.textContent || '').toLocaleLowerCase('pt-BR');
    const challengeMarker = [
      'não é um robô', 'nao e um robo', 'verificação de segurança', 'verificacao de seguranca',
      'security verification', 'captcha', 'robot check',
    ].some((marker) => bodyText.includes(marker));
    const verification = anuncios.length === 0 && !product && challengeMarker;
    const loginPrompt = [
      'para continuar, acesse sua conta', 'já tenho conta', 'ja tenho conta',
      'digite seu e-mail ou telefone', 'acesse sua conta',
    ].some((marker) => bodyText.includes(marker));
    const loginForm = document.querySelector(
      'input[type="email"], input[type="password"], input[name="user_id"], input[name="email"]',
    ) !== null;
    const loginRequired = anuncios.length === 0 && !product && (loginPrompt || loginForm);
    const notFound = /parece que esta p[áa]gina n[ãa]o existe|p[áa]gina n[ãa]o encontrada|access denied|acesso negado/.test(bodyText);
    const noResults = /n[ãa]o h[áa] an[úu]ncios que correspondem|n[ãa]o encontramos resultados|nenhum resultado para/.test(bodyText);
    if (notFound) return { kind: 'error', error: 'O Mercado Livre não carregou esta página. Tente executar a busca novamente.', verification: false };
    if (loginRequired && !verification) return { kind: 'login_required', login_required: true };
    if (!listPage && product && product.preco > 0) {
      return {
        kind: 'product',
        produto: {
          id: product.id, titulo: product.titulo || document.title, preco: product.preco,
          nota: product.nota, qtd_avaliacoes: product.qtd_avaliacoes, url: product.url || location.href,
          ...(product.gtin ? {gtin:product.gtin} : {}),...(product.marca ? {marca:product.marca} : {}),...(product.modelo ? {modelo:product.modelo} : {}),...(product.frete != null ? {frete:product.frete} : {}),
        },
        estrategias: product.estrategias,
        verification: false,
      };
    }
    if (anuncios.length > 0) {
      return {
        kind: 'list', anuncios: anuncios.map(({ estrategias, ...anuncio }) => anuncio),
        estrategias: anuncios.map(({ id, estrategias }) => ({ id, campos: estrategias })), verification: false,
      };
    }
    if (verification || noResults) return { kind: 'list', anuncios: [], estrategias: [], verification };
    return null;
  };

  let observer;
  let timer;
  const stop = () => { observer?.disconnect(); clearTimeout(timer); };
  window.__garimpoStopExtraction = stop;
  const send = (payload) => {
    stop();
    window.__TAURI__?.event?.emit('garimpo:coleta', payload)
      .catch((error) => console.error('Falha ao enviar a coleta:', error));
  };
  const publish = (payload) => window.__TAURI__?.event?.emit('garimpo:coleta', payload)
    .catch((error) => console.error('Falha ao enviar a coleta:', error));
  const initial = extract();
  if (initial?.login_required) {
    publish(initial);
    observer = new MutationObserver(() => {
      const payload = extract();
      if (payload && !payload.login_required) send(payload);
    });
    observer.observe(document.documentElement, { childList: true, subtree: true });
    return;
  }
  if (initial) { send(initial); return; }
  observer = new MutationObserver(() => { const payload = extract(); if (payload) send(payload); });
  observer.observe(document.documentElement, { childList: true, subtree: true });
  timer = setTimeout(() => send({ kind: 'error', error: 'Não foi possível ler os anúncios da página do Mercado Livre. Nenhum resultado foi salvo.', verification: false }), 15000);
})();
