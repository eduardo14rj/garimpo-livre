(() => {
  // Números monetários de JSON-LD são preservados pelos bytes originais do JSON.
  const exactJson = (raw) => JSON.parse(raw, (_key, value, context) =>
    typeof value === 'number' ? context?.source ?? null : value);
  const text = (node, selector) => node.querySelector(selector)?.textContent?.trim() ?? '';
  const cleanDecimal = (value) => /^\d+(?:\.\d+)?$/.test(String(value ?? '')) ? String(value) : null;
  const money = (value) => {
    const matches = [...String(value ?? '').matchAll(/R\$\s*([\d.]+(?:,\d+)?)/g)];
    return matches.map((match) => match[1].replace(/\./g, '').replace(',', '.')).filter(cleanDecimal);
  };
  const count = (value) => {
    const match = String(value ?? '').trim().match(/^([\d.,]+)\s*(mil|k)?/i);
    if (!match) return null;
    const number = Number(match[1].replace(/\./g, '').replace(',', '.')) * (match[2] ? 1000 : 1);
    return Number.isSafeInteger(number) && number >= 0 && number <= 4294967295 ? number : null;
  };
  const ids = (href) => {
    try {
      const url = new URL(href, location.href);
      if (url.protocol !== 'https:' || url.username || url.password
          || !(url.hostname === 'shopee.com.br' || url.hostname.endsWith('.shopee.com.br'))) return null;
      const id = url.pathname.match(/-i\.(\d+)\.(\d+)(?:\/|$)/) || url.pathname.match(/^\/product\/(\d+)\/(\d+)(?:\/|$)/);
      return id ? { shopId: id[1], itemId: id[2], productLink: url.href } : null;
    } catch { return null; }
  };
  const nodes = [];
  const seen = new Set();
  const add = (item) => {
    const key = `${item?.shopId}:${item?.itemId}`;
    if (!item?.productName?.trim() || !cleanDecimal(item.priceMin) || !cleanDecimal(item.priceMax)) return;
    if (seen.has(key)) {
      const existing = nodes.find((node) => `${node.shopId}:${node.itemId}` === key);
      // JSON-LD preserva preço e catálogo; o card acrescenta vendedor, origem e frete.
      if (item._garimpo) {
        for (const [field, value] of Object.entries(item._garimpo)) {
          if (value !== null && value !== undefined && value !== false && value !== 'unknown') existing._garimpo[field] = value;
        }
      }
      if (!existing.shopName && item.shopName) existing.shopName = item.shopName;
      if (existing.sales === null && item.sales !== null) existing.sales = item.sales;
      return;
    }
    seen.add(key); nodes.push(item);
  };
  const visit = (value) => {
    if (!value || typeof value !== 'object') return;
    if (Array.isArray(value)) { value.forEach(visit); return; }
    const types = [].concat(value['@type'] || []);
    if (types.includes('Product')) {
      const offer = [].concat(value.offers || [])[0];
      const identity = ids(offer?.url || value.url || location.href);
      const minimum = cleanDecimal(offer?.lowPrice ?? offer?.price);
      const maximum = cleanDecimal(offer?.highPrice ?? offer?.price ?? offer?.lowPrice);
      if (identity && offer?.priceCurrency === 'BRL' && minimum && maximum) {
        const price = { ...identity, productName: value.name, priceMin: minimum, priceMax: maximum,
          ratingStar: value.aggregateRating?.ratingValue ?? null, sales: null,
          shopName: offer.seller?.name || '', shopType: [],
          _garimpo: {
            qtd_avaliacoes: count(value.aggregateRating?.reviewCount ?? value.aggregateRating?.ratingCount),
            condicao: /NewCondition$/.test(offer.itemCondition || '') ? 'new'
              : /UsedCondition$/.test(offer.itemCondition || '') ? 'used' : 'unknown',
            gtin: value.gtin13 ?? value.gtin14 ?? value.gtin12 ?? value.gtin8 ?? value.gtin ?? null,
            modelo: value.model ?? value.mpn ?? null,
            marca: typeof value.brand === 'string' ? value.brand : value.brand?.name ?? null,
          } };
        add(price);
      }
    }
    Object.values(value).forEach(visit);
  };
  document.querySelectorAll('script[type="application/ld+json"]').forEach((script) => {
    try { visit(exactJson(script.textContent)); } catch { /* tenta o DOM */ }
  });
  document.querySelectorAll('.shopee-search-item-result__item, .shopee-item-card').forEach((card) => {
    const anchor = card.querySelector('a[href*="-i."], a[href*="/product/"]');
    const identity = anchor && ids(anchor.href);
    if (!identity) return;
    const prices = money(text(card, '[data-sqe="price"], .shopee-item-card__current-price'));
    const priceMin = cleanDecimal(card.getAttribute('data-price-min')) ?? prices[0];
    const priceMax = cleanDecimal(card.getAttribute('data-price-max')) ?? prices[1] ?? priceMin;
    const shipping = text(card, '[data-sqe="shipping"], .shopee-item-card__shipping');
    const ratingText = text(card, '[data-sqe="rating"]') || card.querySelector('.shopee-rating-stars')?.getAttribute('aria-label') || '';
    const rating = ratingText.match(/\d+(?:[.,]\d+)?/)?.[0]?.replace(',', '.') ?? null;
    const shopRating = card.getAttribute('data-shop-rating');
    const condition = text(card, '[data-sqe="condition"], .shopee-item-card__condition');
    const label = card.textContent || '';
    add({ ...identity,
      productName: text(card, '[data-sqe="name"], .shopee-item-card__text-name') || anchor.getAttribute('title') || '',
      priceMin, priceMax, ratingStar: rating,
      sales: count(text(card, '[data-sqe="sales"], .shopee-item-card__sold')),
      shopName: text(card, '[data-sqe="shop-name"], .shopee-item-card__shop'), shopType: [],
      _garimpo: {
        frete: /frete gr[aá]tis/i.test(shipping) ? '0' : money(shipping)[0] ?? null,
        qtd_avaliacoes: count(text(card, '[data-sqe="review-count"]')),
        nota_loja: shopRating === null ? null : Number(shopRating.replace(',', '.')),
        oficial: /loja oficial|shopee mall/i.test(label),
        preferida: /vendedor preferido|loja preferida/i.test(label),
        condicao: /\busado\b/i.test(condition) ? 'used' : /\bnovo\b/i.test(condition) ? 'new' : 'unknown',
      },
    });
  });
  if (nodes.length) return { kind: 'list', data: { data: { productOfferV2: { nodes, pageInfo: { page: 1, limit: nodes.length, hasNextPage: false } } } } };
  const body = (document.body?.innerText || document.body?.textContent || '').toLowerCase();
  if (/captcha|verifique que voc[eê]|verifica[cç][aã]o de seguran[cç]a|verify you are/.test(body)) return { verification: true };
  if (document.querySelector('input[type="password"]') || /fa[cç]a login para continuar/.test(body)) return { login_required: true };
  if (/nenhum resultado encontrado|n[aã]o encontramos resultados|0 resultados/.test(body)) return { kind: 'list', data: { data: { productOfferV2: { nodes: [], pageInfo: { page: 1, limit: 50, hasNextPage: false } } } } };
  return null;
})()
