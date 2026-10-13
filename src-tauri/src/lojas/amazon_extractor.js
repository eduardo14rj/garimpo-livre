(() => {
  const exactJson = (raw) => JSON.parse(raw, (_key,value,context) => typeof value === 'number' ? context?.source ?? null : value);
  const text = (node,selector) => node.querySelector(selector)?.textContent?.trim() ?? '';
  const count = (value) => {
    const match = String(value ?? '').match(/([\d.,]+)\s*(mil|k)?/i);
    if (!match) return null;
    const parsed = Number(match[1].replace(/\./g,'').replace(',','.')) * (match[2] ? 1000 : 1);
    return Number.isSafeInteger(parsed) && parsed >= 0 && parsed <= 4294967295 ? parsed : null;
  };
  const decimal = (value) => /^\d+(?:\.\d+)?$/.test(String(value ?? '')) ? String(value) : null;
  const money = (value) => {
    const match = String(value ?? '').match(/R\$\s*([\d.]+(?:,\d+)?)/);
    return match ? decimal(match[1].replace(/\./g,'').replace(',','.')) : null;
  };
  const identity = (href,asin) => {
    try {
      const url = new URL(href,location.href);
      if (url.protocol !== 'https:' || url.username || url.password
        || !(url.hostname === 'amazon.com.br' || url.hostname.endsWith('.amazon.com.br'))) return null;
      const id = asin || url.pathname.match(/\/(?:dp|gp\/product)\/([A-Z0-9]{10})(?:\/|$)/i)?.[1];
      return /^[A-Z0-9]{10}$/i.test(id ?? '') ? { asin:id,detailPageURL:url.href } : null;
    } catch { return null; }
  };
  const items = [];
  const isAmazon = (name) => /^amazon(?:\.com\.br)?$/i.test(String(name ?? '').trim());
  const merchantParts = (merchant) => {
    const segments = merchant.split(/[;\n]/).map((part) => part.trim());
    const sold = segments.map((part) => part.match(/^(?:vendido (?:por|pela)|sold by)\s+(.+)$/i)?.[1]).find(Boolean) || '';
    const combined = segments.some((part) => isAmazon(part.match(/^(?:vendido e (?:enviado|entregue) (?:por|pela)|sold and shipped by)\s+(.+)$/i)?.[1]));
    const shipped = segments.some((part) => isAmazon(part.match(/^(?:enviado (?:por|pela)|entregue (?:por|pela)|ships from)\s+(.+)$/i)?.[1]));
    return { seller:sold,vendido:combined || isAmazon(sold),enviado:combined || shipped };
  };
  const add = (id,name,price,extra={},catalog={}) => {
    if (!id || !name?.trim() || !decimal(price)) return;
    const existing = items.find((item) => item.asin === id.asin);
    if (existing) {
      const seller = existing.offersV2.listings[0].merchantInfo.name;
      // Um ASIN pode ter vendedores diferentes: não misturar preço e entrega de ofertas distintas.
      if (seller && extra.vendedor && seller.toLowerCase() !== extra.vendedor.toLowerCase()) return;
      for (const [key,value] of Object.entries(extra)) if (value !== null && value !== undefined && value !== false) existing._garimpo[key] = value;
      if (extra.vendedor && !existing.offersV2.listings[0].merchantInfo.name) existing.offersV2.listings[0].merchantInfo.name = extra.vendedor;
      return;
    }
    items.push({ ...id, itemInfo:{ title:{displayValue:name},
      byLineInfo:{brand:{displayValue:catalog.marca ?? ''}},manufactureInfo:{model:{displayValue:catalog.modelo ?? ''}},
      externalIds:{eans:{displayValues:catalog.gtin ? [catalog.gtin] : []}} },
      offersV2:{ listings:[{ price:{money:{amount:price,currency:'BRL'}},
        condition:{value:extra.condicao ?? 'Unknown'},merchantInfo:{name:extra.vendedor ?? ''} }] },
      _garimpo:extra,
    });
  };
  const visit = (value) => {
    if (!value || typeof value !== 'object') return;
    if (Array.isArray(value)) { value.forEach(visit); return; }
    if ([].concat(value['@type'] || []).includes('Product')) {
      const offer = [].concat(value.offers || [])[0];
      const id = identity(offer?.url || value.url || location.href,value.asin);
      if (offer?.priceCurrency === 'BRL') {
        const seller = offer.seller?.name || '';
        const shipper = offer.shippingDetails?.shippingOrigin?.name || '';
        const shipping = offer.shippingDetails?.shippingRate;
        add(id,value.name,offer.price,{ vendedor:seller,
          vendido_amazon:isAmazon(seller),enviado_amazon:isAmazon(shipper),
          frete:shipping?.currency === 'BRL' ? decimal(shipping.value) : null,
          nota:value.aggregateRating?.ratingValue === undefined ? null : Number(value.aggregateRating.ratingValue),
          qtd_avaliacoes:count(value.aggregateRating?.reviewCount ?? value.aggregateRating?.ratingCount),
          condicao:/NewCondition$/.test(offer.itemCondition || '') ? 'New' : /UsedCondition$/.test(offer.itemCondition || '') ? 'Used' : 'Unknown',
        },{gtin:value.gtin13 ?? value.gtin14 ?? value.gtin12 ?? value.gtin8 ?? value.gtin,
          marca:typeof value.brand === 'string' ? value.brand : value.brand?.name,modelo:value.model ?? value.mpn});
      }
    }
    Object.values(value).forEach(visit);
  };
  document.querySelectorAll('script[type="application/ld+json"]').forEach((script) => {
    try { visit(exactJson(script.textContent)); } catch { /* cards continuam disponíveis */ }
  });
  document.querySelectorAll('[data-component-type="s-search-result"][data-asin], .amazon-product-card').forEach((card) => {
    const content = card.textContent || '';
    if (/exclusivo.*prime|prime.*exclusive|subscribe\s*(?:&|and)\s*save|programe e poupe/i.test(content)) return;
    const anchor = card.querySelector('a[href*="/dp/"], a[href*="/gp/product/"], h2 a, a:has(h2)');
    const id = anchor && identity(anchor.href,card.getAttribute('data-asin'));
    const current = money(text(card,'[data-sqe="price"], .a-price:not(.a-text-price) .a-offscreen'));
    const rating = text(card,'[data-sqe="rating"], .a-icon-alt').match(/(\d+(?:[.,]\d+)?)\s*(?:de|out of)\s*5/i);
    const shippingText = text(card,'[data-sqe="shipping"], .shipping-price');
    const merchant = text(card,'[data-sqe="merchant"], [data-feature-name="merchantInfo"]');
    const parts = merchantParts(merchant);
    const seller = text(card,'[data-sqe="seller"]') || parts.seller;
    const shipping = /frete gr[aá]tis|entrega gr[aá]tis|free (?:shipping|delivery)/i.test(shippingText) ? '0' : money(shippingText);
    add(id,text(card,'[data-sqe="name"], h2') || anchor?.getAttribute('title'),current,{
      vendedor:seller, vendido_amazon:isAmazon(seller) || (!seller && parts.vendido),
      enviado_amazon:parts.enviado,
      frete:shipping,nota:rating ? Number(rating[1].replace(',','.')) : null,
      qtd_avaliacoes:count(text(card,'[data-sqe="review-count"], a[href*="customerReviews"] span, a[href*="product-reviews"] span')),
      condicao:/\busado\b/i.test(text(card,'[data-sqe="condition"]')) ? 'Used'
        : /\bnovo\b/i.test(text(card,'[data-sqe="condition"]')) ? 'New' : 'Unknown',
    });
  });
  const response = (products) => ({kind:'list',data:{searchResult:{items:products,totalResultCount:products.length}}});
  if (items.length) return response(items);
  const body = (document.body?.innerText || document.body?.textContent || '').toLowerCase();
  if (/captcha|digite os caracteres|enter the characters|verifica[cç][aã]o de seguran[cç]a/.test(body)) return {verification:true};
  if (document.querySelector('input[type="password"]') || /fa[cç]a login para continuar|sign in to continue/.test(body)) return {login_required:true};
  if (/nenhum resultado para|nenhum resultado encontrado|no results for/.test(body)) return response([]);
  return null;
})()
