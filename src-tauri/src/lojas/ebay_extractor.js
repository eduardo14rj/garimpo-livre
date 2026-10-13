(() => {
  const text = (node, selector) => node.querySelector(selector)?.textContent?.trim() ?? '';
  // Valores monetários permanecem strings até o Decimal do Rust.
  const money = (value) => {
    const match = String(value ?? '').match(/R\$\s*([\d.]+(?:,\d+)?)/i);
    return match ? { value: match[1].replace(/\./g, '').replace(',', '.'), currency: 'BRL' } : null;
  };
  const records = [...document.querySelectorAll('.srp-results .s-item, .srp-results .s-card')].flatMap((card) => {
    const anchor = card.querySelector('a.s-item__link, a.s-card__link, a[href*="/itm/"]');
    if (!anchor) return [];
    const title = text(card, '.s-item__title, .s-card__title').replace(/^New Listing\s*/i, '');
    const price = money(text(card, '.s-item__price, .s-card__price, .s-card__attribute-row .su-styled-text'));
    const id = card.getAttribute('data-item-id') || anchor.href.match(/\/itm\/(?:[^/?]+\/)?(\d+)/)?.[1];
    if (!title || !price || !id || /shop on ebay/i.test(title)) return [];
    const shipping = text(card, '.s-item__shipping, .s-item__logisticsCost, .s-card__shipping');
    const destination = (document.body?.innerText || document.body?.textContent || '');
    const brazil = /ship(?:ping)? to\s*:?\s*(?:brazil|brasil)/i.test(destination);
    const shippingCost = brazil
      ? (/frete gr[aá]tis|free shipping|free delivery/i.test(shipping)
        ? { value: '0.00', currency: 'BRL' }
        : money(shipping))
      : null;
    const seller = text(card, '.s-item__seller-info-text, .s-card__seller-info');
    const sellerParts = /^(.*?)\s*\(([\d,]+)\)\s*([\d.]+)%/.exec(seller);
    const condition = text(card, '.SECONDARY_INFO, .s-card__subtitle');
    const url = new URL(anchor.href, location.href);
    const variation = url.searchParams.get('var') || '0';
    return [{
      itemId: id.startsWith('v1|') ? id : `v1|${id}|${variation}`,
      title, itemWebUrl: url.href, price, condition,
      conditionId: /^new$/i.test(condition) ? '1000' : /used|pre-owned/i.test(condition) ? '3000' : null,
      seller: {
        username: sellerParts?.[1] ?? seller,
        feedbackPercentage: sellerParts?.[3] ?? null,
        feedbackScore: sellerParts ? Number(sellerParts[2].replace(/,/g, '')) : null,
      },
      shippingOptions: shippingCost ? [{ shippingCost }] : [],
    }];
  });
  if (records.length) return { kind: 'list', data: { total: records.length, itemSummaries: records } };
  const body = (document.body?.innerText || document.body?.textContent || '').toLowerCase();
  if (/no exact matches found|no results found|0 results for/.test(body)) return { kind: 'list', data: { total: 0, itemSummaries: [] } };
  if (/captcha|verify yourself|verify you are|security measure|pardon our interruption/.test(body)) return { verification: true };
  if (document.querySelector('input[type="password"], input[name="userid"]')) return { login_required: true };
  return null;
})()
