// Docs gallery helper. Not part of the app.
// 1. Inlines icons.svg so <use href="#i-…"> works from file:// and in every webview.
// 2. For every .gx-pair with data-mirror, clones the first pane as a dark copy.
// 3. Applies ?lang=de|tr using handoff/strings/*.json (data-i18n="key").
(async function () {
  try {
    const r = await fetch(new URL('icons.svg', document.currentScript ? document.currentScript.src : location.href));
    const t = await r.text();
    const d = document.createElement('div');
    d.style.cssText = 'position:absolute;width:0;height:0;overflow:hidden';
    d.setAttribute('aria-hidden', 'true');
    d.innerHTML = t;
    document.body.prepend(d);
  } catch (e) { console.warn('icons.svg not loaded', e); }

  document.querySelectorAll('.gx-pair[data-mirror]').forEach(p => {
    const a = p.querySelector('.gx-pane');
    if (!a) return;
    a.setAttribute('data-theme', 'light');
    const b = a.cloneNode(true);
    b.setAttribute('data-theme', 'dark');
    const cap = b.querySelector(':scope > .gx-cap');
    if (cap) cap.textContent = cap.textContent.replace('Light', 'Dark');
    p.appendChild(b);
  });

  document.querySelectorAll('.gx-frame[data-dark]').forEach(f => {
    const w = f.querySelector('.gx-window');
    if (w) w.setAttribute('data-theme', 'light');
    const c = f.cloneNode(true);
    c.removeAttribute('data-dark');
    const cw = c.querySelector('.gx-window');
    if (cw) cw.setAttribute('data-theme', 'dark');
    const cap = c.querySelector('.gx-cap');
    if (cap) cap.textContent += ' · dark';
    c.querySelectorAll('[id]').forEach(n => n.id += '-d');
    f.after(c);
  });

  const langs = document.querySelectorAll('[data-lang]');
  if (langs.length) {
    const cache = {};
    for (const el of langs) {
      const lang = el.getAttribute('data-lang');
      if (lang === 'en') continue;
      if (!cache[lang]) {
        try { cache[lang] = await (await fetch(new URL('strings/' + lang + '.json', location.href))).json(); }
        catch (e) { console.warn('strings missing', lang); cache[lang] = {}; }
      }
      const s = cache[lang];
      el.setAttribute('lang', lang);
      el.querySelectorAll('[data-i18n]').forEach(n => {
        const k = n.getAttribute('data-i18n');
        if (s[k] != null) n.textContent = s[k];
      });
      el.querySelectorAll('[data-i18n-label]').forEach(n => {
        const k = n.getAttribute('data-i18n-label');
        if (s[k] != null) n.setAttribute('aria-label', s[k]);
      });
    }
  }
})();
