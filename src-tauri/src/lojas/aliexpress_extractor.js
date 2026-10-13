(() => {
  const exactJson = (raw) => JSON.parse(raw, (_key, value, context) => typeof value === 'number' ? context?.source ?? null : value);
  const text = (node, selector) => node.querySelector(selector)?.textContent?.trim() ?? '';
  const decimal = (value) => /^\d+(?:\.\d+)?$/.test(String(value ?? '')) ? String(value) : null;
  const price = (value) => {
    const source = String(value ?? '');
    const brl = source.match(/R\$\s*([\d.]+(?:,\d+)?)/);
    if (brl) return { value: brl[1].replace(/\./g, '').replace(',', '.'), currency:'BRL' };
    return null;
  };
  const count = (value) => {
    const match = String(value ?? '').match(/([\d.,]+)\s*(mil|k)?/i);
    if (!match) return null;
    const parsed = Number(match[1].replace(/\./g,'').replace(',','.')) * (match[2] ? 1000 : 1);
    return Number.isSafeInteger(parsed) && parsed >= 0 && parsed <= 4294967295 ? parsed : null;
  };
  const identify = (href) => {
    try {
      const url = new URL(href, location.href);
      if (url.protocol !== 'https:' || url.username || url.password || !['aliexpress.com','aliexpress.us']
        .some((host) => url.hostname === host || url.hostname.endsWith(`.${host}`))) return null;
      const id = url.pathname.match(/\/item\/(\d+)\.html/)?.[1];
      return id ? { product_id:id, product_detail_url:url.href } : null;
    } catch { return null; }
  };
  const products = [];
  const add = (product) => {
    if (!product?.product_title?.trim() || !decimal(product.target_sale_price)
      || product.target_sale_price_currency !== 'BRL') return;
    const existing = products.find((item) => item.product_id === product.product_id);
    if (existing) {
      for (const [key,value] of Object.entries(product._garimpo || {})) {
        if (value !== null && value !== undefined && value !== 'unknown') existing._garimpo[key] = value;
      }
      if (!existing.shop_name && product.shop_name) existing.shop_name = product.shop_name;
      if (existing.lastest_volume === null && product.lastest_volume !== null) existing.lastest_volume = product.lastest_volume;
    } else products.push(product);
  };
  const visit = (value) => {
    if (!value || typeof value !== 'object') return;
    if (Array.isArray(value)) { value.forEach(visit); return; }
    if ([].concat(value['@type'] || []).includes('Product')) {
      const offer = [].concat(value.offers || [])[0];
      const identity = identify(offer?.url || value.url || location.href);
      if (identity && decimal(offer?.price) && offer.priceCurrency === 'BRL') {
        add({ ...identity, product_title:value.name, target_sale_price:String(offer.price),
          target_sale_price_currency:offer.priceCurrency, shop_name:offer.seller?.name || '', lastest_volume:null,
          _garimpo:{ nota_produto:value.aggregateRating?.ratingValue === undefined ? null : Number(value.aggregateRating.ratingValue),
            qtd_avaliacoes:count(value.aggregateRating?.reviewCount ?? value.aggregateRating?.ratingCount),
            gtin:value.gtin13 ?? value.gtin14 ?? value.gtin12 ?? value.gtin8 ?? value.gtin ?? null,
            modelo:value.model ?? value.mpn ?? null, marca:typeof value.brand === 'string' ? value.brand : value.brand?.name ?? null,
            condicao:/NewCondition$/.test(offer.itemCondition || '') ? 'new' : /UsedCondition$/.test(offer.itemCondition || '') ? 'used' : 'unknown',
          } });
      }
    }
    Object.values(value).forEach(visit);
  };
  document.querySelectorAll('script[type="application/ld+json"]').forEach((script) => {
    try { visit(exactJson(script.textContent)); } catch { /* tenta os cards */ }
  });
  document.querySelectorAll('[data-product-id], [class*="list--gallery"], [class*="search-card-item"], .ali-product-card').forEach((card) => {
    const anchor = card.matches('a') ? card : card.querySelector('a[href*="/item/"]');
    const identity = anchor && identify(anchor.href);
    if (!identity) return;
    const current = price(text(card,'[data-sqe="price"], .price-current, [class*="price-sale"]'));
    if (!current) return;
    const shippingText = text(card,'[data-sqe="shipping"], .shipping-price');
    const shipping = /frete gr[aá]tis|free shipping/i.test(shippingText) ? { value:'0', currency:current.currency } : price(shippingText);
    const positive = text(card,'[data-sqe="shop-positive"], .shop-positive-feedback').match(/(\d+(?:[.,]\d+)?)\s*%/);
    const rating = text(card,'[data-sqe="rating"], .product-rating').match(/\d+(?:[.,]\d+)?/);
    const condition = text(card,'[data-sqe="condition"], .product-condition');
    add({ ...identity,
      product_title:text(card,'[data-sqe="name"], .product-title, [class*="title--"]') || anchor.getAttribute('title') || '',
      target_sale_price:current.value, target_sale_price_currency:current.currency,
      shop_name:text(card,'[data-sqe="shop-name"], .shop-name'),
      lastest_volume:count(text(card,'[data-sqe="sales"], .product-sales')),
      _garimpo:{ frete:shipping?.value ?? null, frete_moeda:shipping?.currency ?? null,
        percentual_positivo_loja:positive ? Number(positive[1].replace(',','.')) : null,
        nota_produto:rating ? Number(rating[0].replace(',','.')) : null,
        qtd_avaliacoes:count(text(card,'[data-sqe="review-count"]')),
        condicao:/\busado\b|\bused\b/i.test(condition) ? 'used' : /\bnovo\b|\bnew\b/i.test(condition) ? 'new' : 'unknown',
      },
    });
  });
  const response = (items) => ({ kind:'list', data:{ aliexpress_affiliate_product_query_response:{ resp_result:{
    resp_code:200, result:{ current_page_no:1,total_page_no:1,total_record_count:items.length,products:{ product:items } },
  } } } });
  if (products.length) return response(products);
  const body = (document.body?.innerText || document.body?.textContent || '').toLowerCase();
  if (/captcha|verify you are|verifica[cç][aã]o de seguran[cç]a|slide to verify/.test(body)) return { verification:true };
  if (document.querySelector('input[type="password"]') || /sign in to continue|fa[cç]a login para continuar/.test(body)) return { login_required:true };
  if (/nenhum resultado encontrado|no results found|0 resultados/.test(body)) return response([]);
  return null;
})()
