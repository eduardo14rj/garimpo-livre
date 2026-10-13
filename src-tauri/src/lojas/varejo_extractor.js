(() => {
  const host = location.hostname;
  const loja = host === 'kabum.com.br' || host.endsWith('.kabum.com.br') ? 'kabum'
    : host === 'pichau.com.br' || host.endsWith('.pichau.com.br') ? 'pichau' : null;
  if (!loja) return null;
  const domain = `${loja}.com.br`;
  const storeName = loja === 'kabum' ? 'KaBuM!' : 'Pichau';
  const exactJson = (raw) => JSON.parse(raw,(_key,value,context) => typeof value === 'number' ? context?.source ?? null : value);
  const text = (node,selector) => node.querySelector(selector)?.textContent?.trim() ?? '';
  const decimal = (value) => /^\d+(?:\.\d+)?$/.test(String(value ?? '')) ? String(value) : null;
  const money = (value) => {
    const match = String(value ?? '').match(/R\$\s*([\d.,]+)/);
    if (!match) return null;
    const raw = match[1];
    return decimal(raw.includes(',') ? raw.replace(/\./g,'').replace(',','.') : raw);
  };
  const count = (value) => {
    const parsed = Number(String(value ?? '').replace(/[^\d]/g,''));
    return String(value ?? '').match(/\d/) && Number.isSafeInteger(parsed) && parsed <= 4294967295 ? parsed : null;
  };
  const identity = (href) => {
    try {
      const url = new URL(href,location.href);
      if (url.protocol !== 'https:' || url.username || url.password || !(url.hostname === domain || url.hostname.endsWith(`.${domain}`))) return null;
      const path = url.pathname.replace(/\/$/,'');
      const id = loja === 'kabum' ? path.match(/^\/produto\/(\d+)(?:\/|$)/)?.[1]
        : path.split('/').filter(Boolean).length === 1 && !['search','busca','checkout','customer'].includes(path.slice(1)) ? path.slice(1) : null;
      return id ? {id_externo:id,url:url.origin+path} : null;
    } catch { return null; }
  };
  const products = new Map();
  const blocked = new Set();
  let recognized = false;
  const direct = (seller) => loja === 'kabum' ? /^kabum!$/i.test(seller ?? '') : /^pichau(?: inform[aá]tica)?$/i.test(seller ?? '');
  const add = (candidate,priority=1,available=true) => {
    if (!candidate?.id_externo || !candidate.titulo?.trim()) return;
    recognized = true;
    if (!available) { blocked.add(candidate.id_externo); products.delete(candidate.id_externo); return; }
    if (blocked.has(candidate.id_externo) || !decimal(candidate.preco)) return;
    const prior = products.get(candidate.id_externo);
    if (!prior) products.set(candidate.id_externo,{value:candidate,priority});
    else {
      const sameSeller = !prior.value.vendedor || !candidate.vendedor || prior.value.vendedor.toLowerCase() === candidate.vendedor.toLowerCase();
      if (!sameSeller && priority <= prior.priority) return;
      if (!sameSeller) { products.set(candidate.id_externo,{value:candidate,priority}); return; }
      for (const [key,value] of Object.entries(candidate)) {
        if (value === null || value === undefined || value === '' || value === 'unknown') continue;
        if (['preco','preco_parcelado'].includes(key) && priority < prior.priority) continue;
        prior.value[key] = value;
      }
      prior.priority = Math.max(priority,prior.priority);
    }
  };
  const base = (id,name,price) => ({...id,titulo:name,preco:price,frete:null,preco_parcelado:null,
    gtin:null,modelo:null,marca:null,vendedor:'',venda_direta:false,condicao:'unknown',nota:null,qtd_avaliacoes:null});
  const visitLd = (value) => {
    if (!value || typeof value !== 'object') return;
    if (Array.isArray(value)) { value.forEach(visitLd); return; }
    if ([].concat(value['@type'] || []).includes('Product')) {
      const offer = [].concat(value.offers || [])[0];
      const id = identity(offer?.url || value.url || location.href);
      if (id && offer?.priceCurrency === 'BRL') {
        const specifications = [].concat(offer.priceSpecification || []);
        const cash = specifications.find((spec) => /pix|boleto|[aà] vista/i.test(spec.name || spec.paymentMethod || '') && (!spec.priceCurrency || spec.priceCurrency === 'BRL'));
        const installment = specifications.find((spec) => /parcelado|cart[aã]o/i.test(spec.name || ''));
        const seller = offer.seller?.name || '';
        const shipping = [].concat(offer.shippingDetails || [])[0]?.shippingRate;
        add({...base(id,value.name,decimal(cash?.price ?? offer.price)),
          preco_parcelado:decimal(installment?.price),vendedor:seller,venda_direta:direct(seller),
          frete:shipping?.currency === 'BRL' ? decimal(shipping.value) : null,
          gtin:value.gtin13 ?? value.gtin14 ?? value.gtin12 ?? value.gtin8 ?? value.gtin ?? null,
          modelo:value.model ?? value.mpn ?? null,marca:typeof value.brand === 'string' ? value.brand : value.brand?.name ?? null,
          nota:value.aggregateRating?.ratingValue === undefined ? null : Number(value.aggregateRating.ratingValue),
          qtd_avaliacoes:count(value.aggregateRating?.reviewCount ?? value.aggregateRating?.ratingCount),
          condicao:/NewCondition$/.test(offer.itemCondition || '') ? 'new' : /UsedCondition$/.test(offer.itemCondition || '') ? 'used' : 'unknown',
        },cash ? 3 : 1,!/OutOfStock|Discontinued|PreOrder|SoldOut/.test(offer.availability || ''));
      }
    }
    Object.values(value).forEach(visitLd);
  };
  document.querySelectorAll('script[type="application/ld+json"]').forEach((script) => {
    try { visitLd(exactJson(script.textContent)); } catch { /* próxima estratégia */ }
  });
  const visitState = (value) => {
    if (!value || typeof value !== 'object') return;
    if (Array.isArray(value)) { value.forEach(visitState); return; }
    const name = value.name ?? value.title;
    const cash = decimal(value.prices?.priceWithDiscount ?? value.priceWithDiscount ?? value.cash_price ?? value.price_cash);
    const generic = decimal(value.prices?.price ?? value.price ?? value.price_range?.minimum_price?.final_price?.value);
    let href = value.url ?? value.product_url ?? (value.url_key ? `/${value.url_key}` : null);
    if (!href && loja === 'kabum' && (value.code || value.id) && value.friendlyName) href = `/produto/${value.code ?? value.id}/${value.friendlyName}`;
    if (!href && loja === 'kabum' && value.id && identity(location.href)?.id_externo === String(value.id)) href = location.href;
    const id = href && identity(href);
    if (id && name && (cash || generic)) {
      const seller = value.sellerName ?? value.seller?.name ?? (loja === 'pichau' && value.is_marketplace === false ? storeName : '');
      const generalOffer = value.offer && value.offer.isLoggedUserExclusive === false && !value.offer.isPrimeExclusive && value.offer.quantityAvailable !== '0' ? decimal(value.offer.priceWithDiscount) : null;
      add({...base(id,name,generalOffer ?? cash ?? generic),preco_parcelado:decimal(value.prices?.price ?? value.price),
        vendedor:seller,venda_direta:direct(seller) && value.flags?.isMarketplace !== true && value.is_marketplace !== true,
        frete:value.flags?.isFreeShipping === true || value.free_shipping === true ? '0' : decimal(value.shipping_price),
        gtin:value.gtin ?? value.ean ?? null,modelo:value.model ?? value.mpn ?? null,
        marca:value.manufacturer?.name ?? value.brand?.name ?? (typeof value.brand === 'string' ? value.brand : null),
        nota:value.averageRating === undefined && value.rating === undefined ? null : Number(value.averageRating ?? value.rating),
        qtd_avaliacoes:count(value.ratingCount ?? value.review_count),
        condicao:value.flags?.isOpenbox === true ? 'used' : value.condition === 'new' || value.flags?.isOpenbox === false ? 'new' : 'unknown',
      },cash || generalOffer ? 4 : 2,value.available !== false && value.is_in_stock !== false && value.stock_status !== 'OUT_OF_STOCK');
    }
    Object.values(value).forEach(visitState);
  };
  document.querySelectorAll('script#__NEXT_DATA__, script#__INITIAL_STATE__, script[type="application/json"][data-product-state]').forEach((script) => {
    try { visitState(exactJson(script.textContent)); } catch { /* DOM ainda pode responder */ }
  });
  const field = (node,name) => {
    const explicit = node.querySelector(`[data-field="${name}"], [itemprop="${name}"]`);
    if (explicit) return explicit.getAttribute('content') || explicit.textContent.trim();
    for (const row of node.querySelectorAll('tr')) {
      const cells = [...row.querySelectorAll('th,td')];
      if (cells.length >= 2 && new RegExp(`^${name}:?$`,'i').test(cells[0].textContent.trim())) return cells[1].textContent.trim();
    }
    return null;
  };
  const readCard = (card,href) => {
    const id = identity(href);
    if (!id) return;
    const cashNode = card.querySelector('[data-price-cash], [data-sqe="cash-price"], .priceCard, .price-cash');
    const content = card.textContent || '';
    const cashLabel = content.match(/(?:[aà] vista|no pix|no boleto)\s*(R\$\s*[\d.,]+)/i)?.[1]
      || content.match(/(R\$\s*[\d.,]+)\s*(?:[aà] vista|no pix|no boleto)/i)?.[1];
    const genericPrice = text(card,'[data-sqe="price"], [itemprop="price"], .product-price');
    const current = cashNode ? decimal(cashNode.getAttribute('data-price-cash')) || money(cashNode.textContent)
      : cashLabel ? money(cashLabel) : /\d+\s*x|parcela/i.test(genericPrice) ? null : money(genericPrice);
    const installment = card.querySelector('[data-price-installment], [data-sqe="installment-total"], .price-installment');
    const seller = text(card,'[data-sqe="seller"], .seller-name') || card.getAttribute('data-seller')
      || (loja === 'pichau' && !/marketplace|vendido (?:por|pela)\s+/i.test(content) ? storeName : '');
    const condition = text(card,'[data-sqe="condition"]');
    const shippingText = text(card,'[data-sqe="shipping"], .shipping-price');
    const rating = text(card,'[data-sqe="rating"], [itemprop="ratingValue"]') || card.querySelector('[itemprop="ratingValue"]')?.getAttribute('content');
    const title = text(card,'h1, h2, h3, [data-sqe="name"], .nameCard, [itemprop="name"]');
    const installmentTotal = content.match(/(R\$\s*[\d.,]+)\s*em(?: at[eé])?\s*\d+\s*x/i)?.[1];
    add({...base(id,title,current),preco_parcelado:installment ? decimal(installment.getAttribute('data-price-installment')) || money(installment.textContent) : money(installmentTotal),
      vendedor:seller,venda_direta:direct(seller),
      frete:/frete gr[aá]tis|entrega gr[aá]tis/i.test(shippingText) ? '0' : money(shippingText),
      gtin:field(card,'gtin13') ?? field(card,'gtin') ?? field(card,'EAN'),modelo:field(card,'modelo'),marca:field(card,'marca'),
      nota:rating?.match(/\d+(?:[.,]\d+)?/) ? Number(rating.match(/\d+(?:[.,]\d+)?/)[0].replace(',','.')) : null,
      qtd_avaliacoes:count(text(card,'[data-sqe="review-count"], [itemprop="reviewCount"]')),
      condicao:/usado|openbox|recondicionado/i.test(condition) ? 'used' : /novo/i.test(condition) ? 'new' : 'unknown',
    },cashNode || cashLabel ? 5 : 1,!/produto (?:esgotado|indispon[ií]vel)/i.test(content));
  };
  document.querySelectorAll('article[data-product-id], .productCard, .retail-product-card').forEach((card) => {
    const anchor = card.matches('a') ? card : card.querySelector('a[href]');
    if (anchor) readCard(card,anchor.href);
  });
  // Links/títulos e rótulos de pagamento continuam utilizáveis quando classes de CSS mudam.
  document.querySelectorAll('a[href]').forEach((anchor) => {
    if (!identity(anchor.href)) return;
    const card = anchor.closest('article, [data-product-id], .productCard, .MuiCard-root') || anchor;
    if (card.querySelector('h2,h3,[data-sqe="name"]')) readCard(card,anchor.href);
  });
  const current = identity(location.href);
  if (current && document.querySelector('h1')) readCard(document.querySelector('main') || document.body,location.href);
  const clean = (entry) => entry.value;
  if (current) {
    const product = products.get(current.id_externo);
    if (product) return {kind:'product',data:clean(product)};
    if (blocked.has(current.id_externo)) return {error:'Produto indisponível nesta loja.'};
  } else if (products.size || recognized) return {kind:'list',data:[...products.values()].map(clean)};
  const body = (document.body?.innerText || document.body?.textContent || '').toLowerCase();
  if (/captcha|verify you are|verifica[cç][aã]o de seguran[cç]a|just a moment/.test(body)) return {verification:true};
  if (document.querySelector('input[type="password"]')) return {login_required:true};
  if (!current && /nenhum produto encontrado|nenhum resultado|0 produtos/.test(body)) return {kind:'list',data:[]};
  return null;
})()
