(function () {
'use strict';

const THEME_KEY = 'fieldbook.theme';
const WORDING_KEY = 'fieldbook.wording';

const ROUTE_SECTIONS = {
  'index.html': { section: 'today', item: 'today' },
  'index.html#areas': { section: 'today', item: 'today' },
  'map.html': { section: 'today', item: 'map' },
  'about.html': { section: 'today', item: 'today' },
  'changes.html': { section: 'today', item: 'today' },
  'assistant.html': { section: 'assistant', item: 'assistant' },
  'a/<area>.html': { section: 'areas', item: 'area-<area>' },
  'p/<page-id>.html': { section: 'areas', item: 'area-<area>' },
  'day-one.html': { section: 'library', item: 'templates' },
  'claims.html': { section: 'library', item: 'claims' },
  'questions.html': { section: 'library', item: 'questions' },
  'conflicts.html': { section: 'library', item: 'conflicts' },
  'glossary.html': { section: 'library', item: 'glossary' },
  'sources.html': { section: 'library', item: 'sources' },
  's/<version-id>.html': { section: 'library', item: 'sources' },
  'ids.html': { section: 'library', item: 'browse' },
  'search.html': { section: 'library', item: 'browse' }
};

let sessionTheme = null;
let toastTimer = null;
let lastTrigger = null;
let slideTimer = null;

// The frame element with id `fb:<name>`, or null.
function fb(name) {
  return document.getElementById('fb:' + name);
}

// A string from window.FB_STRINGS, or ''.
function str(name) {
  return (window.FB_STRINGS || {})[name] || '';
}

// The element matching `sel` at or above the event's target, or null.
function closest(ev, sel) {
  return ev.target && ev.target.closest ? ev.target.closest(sel) : null;
}

function showToast(message) {
  const toast = fb('toast');
  const sr = fb('sr');
  if (!message) {
    return;
  }
  if (sr) {
    sr.textContent = message;
  }
  if (toast) {
    toast.textContent = message;
    toast.hidden = false;
    toast.classList.add('show');
    clearTimeout(toastTimer);
    toastTimer = setTimeout(function () {
      toast.classList.remove('show');
      toast.hidden = true;
    }, 3200);
  }
}

function effectiveTheme() {
  const theme = document.documentElement.getAttribute('data-theme') || sessionTheme;
  if (theme === 'dark' || theme === 'light') {
    return theme;
  }
  return window.matchMedia && matchMedia('(prefers-color-scheme: dark)').matches
    ? 'dark'
    : 'light';
}

function applyTheme(theme) {
  const btn = fb('themeBtn');
  sessionTheme = theme;
  if (theme) {
    document.documentElement.setAttribute('data-theme', theme);
  }
  if (btn) {
    btn.setAttribute('aria-pressed', String(effectiveTheme() === 'dark'));
  }
}

function initTheme() {
  let stored = null;
  try {
    stored = window.localStorage.getItem(THEME_KEY);
  } catch (_err) {}
  applyTheme(stored === 'dark' || stored === 'light' ? stored : null);
}

function toggleTheme() {
  const next = effectiveTheme() === 'dark' ? 'light' : 'dark';
  applyTheme(next);
  try {
    window.localStorage.setItem(THEME_KEY, next);
  } catch (_err) {
    showToast(str('not_remembered'));
  }
}

function selectSidebarSection(secKey) {
  document.querySelectorAll('.sidebar .nav-group[data-sec]').forEach(function (group) {
    group.hidden = group.getAttribute('data-sec') !== secKey;
  });
  document.querySelectorAll('#fb\\:secSwitch [data-secpick]').forEach(function (btn) {
    btn.setAttribute('aria-pressed', String(btn.getAttribute('data-secpick') === secKey));
  });
}

function isOverlayOpen() {
  const sidebar = fb('sidebar');
  return !!sidebar && sidebar.classList.contains('open');
}

// Opens or closes the overlay sidebar (below 960px), its scrim and the menu button.
function setSidebar(open, restoreFocus) {
  const sidebar = fb('sidebar');
  const scrim = fb('scrim');
  const menuBtn = fb('menuBtn');
  if (!sidebar || open === isOverlayOpen()) {
    return;
  }
  if (open) {
    lastTrigger = document.activeElement;
  }
  sidebar.classList.toggle('open', open);
  // The slide is decoration: where the animation clock stops, end it so the sidebar shows.
  clearTimeout(slideTimer);
  slideTimer = setTimeout(function () {
    sidebar.getAnimations().forEach(function (a) { a.finish(); });
  }, 400);
  if (scrim) {
    scrim.hidden = !open;
    scrim.classList.toggle('open', open);
  }
  if (menuBtn) {
    menuBtn.setAttribute('aria-expanded', String(open));
  }
  const target = open
    ? sidebar.querySelector('button:not([hidden]), a[href]:not([hidden]), [tabindex]:not([tabindex="-1"])')
    : restoreFocus && (lastTrigger || menuBtn);
  if (target) {
    target.focus();
  }
}

function initFrame() {
  initTheme();

  const themeBtn = fb('themeBtn');
  if (themeBtn) {
    themeBtn.addEventListener('click', toggleTheme);
  }

  const menuBtn = fb('menuBtn');
  if (menuBtn) {
    menuBtn.addEventListener('click', function () {
      setSidebar(!isOverlayOpen(), true);
    });
  }
  [fb('sidebarCloseBtn'), fb('scrim')].forEach(function (el) {
    if (el) {
      el.addEventListener('click', function () {
        setSidebar(false, true);
      });
    }
  });

  const secSwitch = fb('secSwitch');
  if (secSwitch) {
    secSwitch.addEventListener('click', function (ev) {
      const btn = closest(ev, '[data-secpick]');
      if (btn) {
        selectSidebarSection(btn.getAttribute('data-secpick'));
      }
    });
  }

  document.addEventListener('keydown', function (ev) {
    if (ev.key === 'Escape' && isOverlayOpen() && !document.querySelector('dialog[open]')) {
      ev.preventDefault();
      setSidebar(false, true);
    }
  });

  if (window.matchMedia) {
    matchMedia('(min-width: 960px)').addEventListener('change', function (e) {
      if (e.matches) {
        setSidebar(false, false);
      }
    });
  }

  const appRoot = document.querySelector('.app[data-route]');
  if (appRoot) {
    const route = appRoot.getAttribute('data-route') || 'index.html';
    const mapped = ROUTE_SECTIONS[route];
    if (mapped && !appRoot.getAttribute('data-section')) {
      selectSidebarSection(mapped.section);
    }
  }

  initBuildAge();
  initCommandChips();
  initPageReader();
  initRegister();
  initPalette();
}

function localIsoDate() {
  const now = new Date();
  const pad = function (n) {
    return String(n).padStart(2, '0');
  };
  return now.getFullYear() + '-' + pad(now.getMonth() + 1) + '-' + pad(now.getDate());
}

function applyWording(mode) {
  const reader = fb('reader') || document.body;
  reader.classList.toggle('hide-precise', mode === 'plain');
  document.querySelectorAll('#fb\\:wordingSeg [data-wording]').forEach(function (btn) {
    btn.setAttribute('aria-pressed', String(btn.getAttribute('data-wording') === mode));
  });
}

function showPopoverFor(el) {
  const pop = fb('popover');
  if (!pop || !el) {
    return;
  }
  const title = el.getAttribute('data-pv-title') || '';
  let body = el.getAttribute('data-pv-body') || '';
  const pv = el.getAttribute('data-pv') || '';
  if (!body && pv.indexOf('cite:') === 0) {
    const row = document.getElementById('cite-' + pv.slice(5));
    body = row ? row.textContent.trim() : '';
  }
  pop.textContent = '';
  if (title) {
    pop.append(make('div', 'popover-title', title));
  }
  if (body) {
    pop.append(make('div', 'popover-body', body));
  }
  pop.hidden = !title && !body;
}

function hidePopover() {
  const pop = fb('popover');
  if (pop) {
    pop.hidden = true;
  }
}

function editionAge(iso, now) {
  const parts = /^(\d{4})-(\d{2})-(\d{2})$/.exec(iso);
  if (!parts) {
    throw new Error('no build date');
  }
  const built = Date.UTC(Number(parts[1]), Number(parts[2]) - 1, Number(parts[3]));
  const days = Math.floor((now.getTime() - built) / 86400000);
  if (!Number.isFinite(days)) {
    throw new Error('no build date');
  }
  if (days <= 0) {
    return 'today';
  }
  if (days === 1) {
    return '1 day ago';
  }
  if (days < 60) {
    return days + ' days ago';
  }
  return days < 730
    ? Math.round(days / 30.4) + ' months ago'
    : Math.round(days / 365) + ' years ago';
}

function initBuildAge() {
  const stamp = document.querySelector('.build-date time[datetime]');
  if (!stamp) {
    return;
  }
  try {
    const iso = stamp.getAttribute('datetime') || '';
    stamp.textContent = editionAge(iso, new Date());
    stamp.title = iso;
  } catch (_err) {
    // The date the build wrote stays as the text.
  }
}

function selectChipText(code) {
  const selection = window.getSelection ? getSelection() : null;
  if (!selection || !document.createRange) {
    return;
  }
  const range = document.createRange();
  range.selectNodeContents(code);
  selection.removeAllRanges();
  selection.addRange(range);
}

// Copy writes only on the reader's click; a refusal selects the text to copy by hand.
function copyChip(btn) {
  const chip = btn.closest('.cmd');
  const code = chip ? chip.querySelector('code') : null;
  if (!code) {
    return;
  }
  const refused = function () {
    btn.focus();
    selectChipText(code);
    showToast('Copy blocked here. The text is selected; copy it by hand.');
  };
  const clip = navigator.clipboard;
  if (!clip || typeof clip.writeText !== 'function') {
    refused();
    return;
  }
  clip.writeText(code.textContent).then(function () {
    showToast(
      btn.getAttribute('data-copy-chip') === 'address'
        ? 'Address copied.'
        : 'Command copied. Run it in a terminal.'
    );
  }, refused);
}

function initCommandChips() {
  document.addEventListener('click', function (ev) {
    const btn = closest(ev, '[data-copy-chip]');
    if (btn) {
      copyChip(btn);
    }
  });
}

function initPageReader() {
  const today = localIsoDate();
  document.querySelectorAll('[data-review-by]').forEach(function (banner) {
    const reviewBy = banner.getAttribute('data-review-by') || '';
    const pageDue = Boolean(reviewBy && today > reviewBy);
    const rows = banner.querySelectorAll('[data-claim-review-by]');
    const dueBlock = banner.querySelector('.due-claims');
    const dueCount = banner.querySelector('.due-count');
    const pageLine = banner.querySelector('.body');
    let dueRows = 0;
    for (let j = 0; j < rows.length; j += 1) {
      const claimBy = rows[j].getAttribute('data-claim-review-by') || '';
      if (claimBy && today > claimBy) {
        rows[j].hidden = false;
        dueRows += 1;
      }
    }
    if (dueBlock) {
      dueBlock.hidden = dueRows === 0;
    }
    if (dueCount) {
      dueCount.textContent = String(dueRows);
    }
    if (pageLine) {
      pageLine.hidden = !pageDue;
    }
    if (pageDue || dueRows > 0) {
      banner.hidden = false;
    }
  });

  const wordingSeg = fb('wordingSeg');
  if (wordingSeg) {
    let storedWording = null;
    try {
      storedWording = window.localStorage.getItem(WORDING_KEY);
    } catch (_err) {}
    if (storedWording === 'plain' || storedWording === 'both') {
      applyWording(storedWording);
    }
    wordingSeg.addEventListener('click', function (ev) {
      const btn = closest(ev, '[data-wording]');
      if (!btn) {
        return;
      }
      const mode = btn.getAttribute('data-wording') === 'plain' ? 'plain' : 'both';
      applyWording(mode);
      try {
        window.localStorage.setItem(WORDING_KEY, mode);
      } catch (_err) {
        showToast(str('not_remembered'));
      }
    });
  }

  const copyBtn = fb('copyLinkBtn');
  if (copyBtn) {
    copyBtn.addEventListener('click', function () {
      const clip = navigator.clipboard;
      const failed = function () {
        showToast('Copy failed');
      };
      if (clip && clip.writeText) {
        clip.writeText(location.href).then(function () {
          showToast('Link copied');
        }, failed);
      } else {
        failed();
      }
    });
  }

  const reader = fb('reader');
  if (reader) {
    reader.addEventListener('click', function (ev) {
      const mark = closest(ev, '[data-claim]');
      if (!mark) {
        return;
      }
      const claimId = mark.getAttribute('data-claim');
      const card = claimId ? document.getElementById('ev-' + claimId) : null;
      if (card) {
        card.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
        card.classList.add('flash');
        setTimeout(function () {
          card.classList.remove('flash');
        }, 1400);
      }
    });

    // Hover or focus on a [data-pv] mark previews it.
    ['mouseover', 'focusin', 'mouseout'].forEach(function (type) {
      reader.addEventListener(type, function (ev) {
        const pv = closest(ev, '[data-pv]');
        if (pv && type === 'mouseout') {
          hidePopover();
        } else if (pv) {
          showPopoverFor(pv);
        }
      });
    });

    reader.addEventListener('focusout', function () {
      hidePopover();
    });

    document.addEventListener('keydown', function (ev) {
      if (ev.key === 'Escape') {
        hidePopover();
      }
    });
  }
}

function initRegister() {
  const PAGE_SIZE = 50;
  const NUMBER = /^-?\d+(\.\d+)?$/;
  const regTable = fb('registerTable');
  const thead = regTable ? regTable.querySelector('thead') : null;
  const tbody = regTable ? regTable.querySelector('tbody') : null;
  if (!regTable || !thead || !tbody) {
    return;
  }

  // Rows in the order they are shown (sorted), and the subset the filter keeps.
  let ordered = Array.prototype.slice.call(tbody.rows);
  let matching = ordered.slice();
  let filterVal = 'all';
  let page = 0;

  const filterBar = fb('claimsFilter');
  const filterSeg = fb('claimsFilterSeg');
  const emptyLine = fb('registerEmptyFilter');
  const pager = fb('registerPager');
  const pageInfo = fb('registerPageInfo');
  const prevBtn = pager ? pager.querySelector('[data-pg="prev"]') : null;
  const nextBtn = pager ? pager.querySelector('[data-pg="next"]') : null;

  const announce = function (text) {
    const sr = fb('sr');
    if (sr) {
      sr.textContent = text;
    }
  };

  // Without scripts the table is complete; with them a register shows 50 rows at a time.
  const render = function () {
    const pages = Math.max(1, Math.ceil(matching.length / PAGE_SIZE));
    page = Math.min(page, pages - 1);
    if (page < 0) {
      page = 0;
    }
    const first = page * PAGE_SIZE;
    const visible = new Set(matching.slice(first, first + PAGE_SIZE));
    ordered.forEach(function (row) {
      if (row.hidden === visible.has(row)) {
        row.hidden = !row.hidden;
      }
    });
    if (pager) {
      pager.hidden = matching.length <= PAGE_SIZE;
      if (pageInfo) {
        pageInfo.textContent = 'Rows ' + (first + 1) + ' to ' +
          Math.min(first + PAGE_SIZE, matching.length) + ' of ' + matching.length;
      }
      if (prevBtn) {
        prevBtn.disabled = page === 0;
      }
      if (nextBtn) {
        nextBtn.disabled = page >= pages - 1;
      }
    }
  };

  const applyFilter = function () {
    matching = ordered.filter(function (row) {
      return filterVal === 'all' || row.getAttribute('data-verification') === filterVal;
    });
    if (filterSeg) {
      filterSeg.querySelectorAll('[data-cf]').forEach(function (btn) {
        btn.setAttribute('aria-pressed', String(btn.getAttribute('data-cf') === filterVal));
      });
    }
    if (emptyLine) {
      emptyLine.hidden = matching.length > 0;
    }
  };

  const collator = new Intl.Collator('en', { numeric: true });

  // Plain numbers compare exactly, as decimal strings: a double cannot tell
  // 0.1234567890123456789 from 0.1234567890123456788. The sign decides first; then the integer
  // part by length and then digit by digit; then the fraction padded to one length.
  // The cases below are checked by a test against a Rust port of this same algorithm, and the
  // whole block is pinned verbatim there: change one, re-check the other.
  //   COMPARE-CASES
  //   0 | 0 | 0
  //   -0 | 0 | 0
  //   0.0 | 0 | 0
  //   007 | 7 | 0
  //   9 | 10 | -1
  //   10 | 9 | 1
  //   -9 | -10 | 1
  //   -10 | 9 | -1
  //   -1 | 1 | -1
  //   1.5 | 1.50 | 0
  //   1.5 | 1.25 | 1
  //   0.5 | 0.25 | 1
  //   -0.5 | -0.25 | -1
  //   -0.5 | 0.25 | -1
  //   0.001 | 0.01 | -1
  //   123456789012345678901234567890 | 123456789012345678901234567891 | -1
  //   0.1234567890123456789 | 0.1234567890123456788 | 1
  //   -0.1234567890123456789 | -0.1234567890123456788 | -1
  //   9007199254740993 | 9007199254740992 | 1
  //   0.30000000000000004 | 0.3 | 1
  //   END-COMPARE-CASES
  const parseDecimal = function (text) {
    let negative = text.charAt(0) === '-';
    const body = negative ? text.slice(1) : text;
    const dot = body.indexOf('.');
    const whole = (dot < 0 ? body : body.slice(0, dot)).replace(/^0+/, '');
    const fraction = dot < 0 ? '' : body.slice(dot + 1).replace(/0+$/, '');
    if (whole === '' && fraction === '') {
      negative = false;
    }
    return { negative: negative, whole: whole, fraction: fraction };
  };
  const compareMagnitude = function (a, b) {
    if (a.whole.length !== b.whole.length) {
      return a.whole.length < b.whole.length ? -1 : 1;
    }
    if (a.whole !== b.whole) {
      return a.whole < b.whole ? -1 : 1;
    }
    const width = Math.max(a.fraction.length, b.fraction.length);
    const fa = a.fraction.padEnd(width, '0');
    const fb = b.fraction.padEnd(width, '0');
    return fa === fb ? 0 : fa < fb ? -1 : 1;
  };
  const compareDecimal = function (x, y) {
    const a = parseDecimal(x);
    const b = parseDecimal(y);
    if (a.negative !== b.negative) {
      return a.negative ? -1 : 1;
    }
    const magnitude = compareMagnitude(a, b);
    return a.negative && magnitude !== 0 ? -magnitude : magnitude;
  };
  // Other cells compare as text with digit runs read numerically.
  const collate = function (x, y) {
    if (NUMBER.test(x) && NUMBER.test(y)) {
      return compareDecimal(x, y);
    }
    return collator.compare(x, y);
  };

  thead.querySelectorAll('th[scope="col"]').forEach(function (th) {
    const colName = th.getAttribute('data-col');
    if (!colName || th.querySelector('[data-sort-col]')) {
      return;
    }
    const btn = make('button', '', colName, { type: 'button', 'data-sort-col': colName });
    const arrow = make('span', 'sort-arrow', null);
    btn.className = 'th-sort';
    arrow.setAttribute('aria-hidden', 'true');
    btn.append(arrow);
    th.textContent = '';
    th.append(btn);
  });

  thead.addEventListener('click', function (ev) {
    const btn = closest(ev, '[data-sort-col]');
    if (!btn) {
      return;
    }
    const colName = btn.getAttribute('data-sort-col');
    const th = btn.closest('th');
    if (!colName || !th) {
      return;
    }
    const nextDir = th.getAttribute('aria-sort') === 'ascending' ? 'descending' : 'ascending';
    const all = thead.querySelectorAll('th[scope="col"]');
    all.forEach(function (h) {
      h.removeAttribute('aria-sort');
    });
    th.setAttribute('aria-sort', nextDir);

    const sign = nextDir === 'ascending' ? 1 : -1;
    const colIndex = Array.prototype.indexOf.call(all, th);
    const keys = new Map();
    ordered.forEach(function (row) {
      const cell = row.cells[colIndex];
      keys.set(row, !cell ? '' : cell.hasAttribute('data-sort-val')
        ? cell.getAttribute('data-sort-val') : cell.textContent.trim());
    });
    ordered.sort(function (a, b) {
      const byVal = collate(keys.get(a), keys.get(b));
      if (byVal !== 0) {
        return byVal * sign;
      }
      return collate(a.id || '', b.id || '') * sign;
    });
    ordered.forEach(function (row) {
      tbody.append(row);
    });
    page = 0;
    applyFilter();
    render();
  });

  if (filterBar && filterSeg) {
    filterBar.hidden = false;
    filterSeg.addEventListener('click', function (ev) {
      const btn = closest(ev, '[data-cf]');
      if (!btn) {
        return;
      }
      filterVal = btn.getAttribute('data-cf') || 'all';
      page = 0;
      applyFilter();
      render();
      announce(
        matching.length > 0
          ? matching.length + ' shown'
          : 'An empty filter means none are recorded, not that none exist.'
      );
    });
  }

  if (pager) {
    pager.addEventListener('click', function (ev) {
      const btn = closest(ev, '[data-pg]');
      if (!btn || btn.disabled) {
        return;
      }
      page += btn.getAttribute('data-pg') === 'next' ? 1 : -1;
      render();
      regTable.parentNode.scrollIntoView({ block: 'start' });
      announce(pageInfo ? pageInfo.textContent : '');
    });
  }

  const rowById = function (id) {
    return ordered.find(function (row) {
      return row.id === id;
    }) || null;
  };

  // A link to a row (`claims.html#CL-...`) must land on that row, whatever page or filter hides it.
  const revealHash = function () {
    let id = '';
    try {
      id = decodeURIComponent(location.hash.replace(/^#/, ''));
    } catch (_err) {}
    // Only the register's own rows answer, never a frame element.
    const row = id ? rowById(id) : null;
    if (!row) {
      return;
    }
    if (matching.indexOf(row) < 0) {
      filterVal = 'all';
      applyFilter();
    }
    const at = matching.indexOf(row);
    if (at < 0) {
      return;
    }
    page = Math.floor(at / PAGE_SIZE);
    render();
    row.scrollIntoView({ block: 'center' });
  };
  window.addEventListener('hashchange', revealHash);

  render();
  revealHash();
}

// SearchTrigger, CommandPalette and ShortcutsDialog: docs/reference/search.md, Palette.
const KEYS_KEY = 'fieldbook.singleKeys';
const FIELDS = 'id kind title parent snippet status as_of effective_from verified_on'.split(' ');
const folds = new WeakMap();
let singleKeys = true;
let keysOpener = null;
let selected = 0;
let root = '';

// Strings among `kids` become text nodes, never markup.
function make(tag, cls, kids, attrs) {
  const e = document.createElement(tag);
  e.className = cls;
  e.append(...[].concat(kids || []));
  Object.keys(attrs || {}).forEach(function (k) {
    e.setAttribute(k, attrs[k]);
  });
  return e;
}

function fold(text) {
  return text.normalize('NFKC').toLowerCase().replace(/[\u2010-\u2015\u2212]/g, '-')
    .replace(/\u00ad/g, '').trim();
}

function asHit(d, u) {
  const h = { unit: u };
  FIELDS.forEach(function (f, i) {
    h[f] = d[i];
  });
  return h;
}

// Id-prefix matches (no section units) in unit order, then unlisted page-view rows; eight at most.
function paletteRows(ix, text) {
  const S = window.FB_SEARCH;
  const q = fold(text);
  const rows = [];
  const add = function (h) {
    if (rows.length < 8 && !rows.some(function (r) { return r.id === h.id; })) {
      rows.push(h);
    }
  };
  if (!/\s/.test(q)) {
    if (!folds.has(ix)) {
      folds.set(ix, ix.docs.map(function (d) { return fold(d[0]); }));
    }
    folds.get(ix).forEach(function (id, u) {
      if (id.indexOf('#') < 0 && id.startsWith(q)) {
        add(asHit(ix.docs[u], u));
      }
    });
  }
  const v = S.view(ix, S.rank(ix, text).hits);
  v.exact.concat(v.pages, v.other).forEach(add);
  return rows;
}

// The six page leads with the latest as_of, compared as written.
function latestPages(ix) {
  return ix.docs.map(asHit).filter(function (h) {
    return /^(page|clause)$/.test(h.kind) && !h.parent && h.as_of;
  }).sort(function (a, b) {
    return a.as_of < b.as_of ? 1 : a.as_of > b.as_of ? -1 : a.unit - b.unit;
  }).slice(0, 6);
}

function select(i) {
  const input = fb('palIn');
  const opts = fb('palList').children;
  selected = Math.max(0, Math.min(i, opts.length - 1));
  input.removeAttribute('aria-activedescendant');
  for (let k = 0; k < opts.length; k += 1) {
    opts[k].id = 'fb:pal-' + k;
    opts[k].setAttribute('aria-selected', String(k === selected));
    if (k === selected) {
      input.setAttribute('aria-activedescendant', opts[k].id);
      opts[k].scrollIntoView({ block: 'nearest' });
    }
  }
}

function paint() {
  const S = window.FB_SEARCH;
  const text = fb('palIn').value;
  const list = fb('palList');
  const note = fb('palNote');
  // Busy, and a Skeleton after 300ms, until the index settles.
  const t = setTimeout(function () {
    list.replaceChildren(make('li', 'skeleton', null, { 'aria-hidden': 'true' }));
  }, 300);
  fb('palette').setAttribute('aria-busy', 'true');
  S.load().finally(function () {
    clearTimeout(t);
    fb('palette').removeAttribute('aria-busy');
  }).then(function (ix) {
    const q = text.trim();
    const rows = q && S.tokens(q).length ? paletteRows(ix, q) : null;
    S.render(rows || latestPages(ix), q, list);
    note.className = rows ? 'pal-msg panel' : 'pal-note';
    note.textContent = rows ? (rows.length ? '' : str('abstention')) : 'Latest pages';
    if (rows) {
      list.append(make('li', 'sr-row pal-search', make('a', 'sr-title', 'Search for ' + q, {
        href: root + 'search.html#q=' + encodeURIComponent(q)
      }), { role: 'option' }));
    }
    select(0);
  }, function (why) {
    const v = why === 'version';
    list.textContent = '';
    note.className = 'pal-msg';
    note.textContent = v ? 'Search is unavailable for this index version. Use the '
      : 'The search index did not load. ';
    note.append(make('a', '', v ? 'ID list' : 'All IDs', { href: root + 'ids.html' }), v ? '.' : '');
  });
}

function openPalette() {
  const dlg = fb('palette');
  const input = fb('palIn');
  const keys = fb('keys');
  if (keys.open) {
    keys.close();
  }
  if (!dlg.open) {
    dlg.showModal();
  }
  input.focus();
  input.select();
  paint();
}

function openKeys() {
  const dlg = fb('keys');
  if (!dlg.open) {
    keysOpener = document.activeElement;
    dlg.showModal();
    dlg.querySelector('button').focus();
  }
}

function initPalette() {
  const link = fb('searchBtn');
  const pal = fb('palette');
  const keys = fb('keys');
  const keysBtn = fb('keysBtn');
  if (!link || !pal || !keys || !window.FB_SEARCH) {
    return;
  }
  let failed = '';
  try {
    singleKeys = window.localStorage.getItem(KEYS_KEY) !== 'off';
  } catch (_err) {
    failed = str('not_remembered');
  }
  root = link.getAttribute('href').replace(/ids\.html$/, '');
  const kbd = function (k) {
    return make('kbd', '', k);
  };
  const trigger = make('button', link.className, [].slice.call(link.childNodes).concat(kbd('Ctrl K')),
    { type: 'button', id: link.id, 'aria-label': 'Search the library (Control K)' });
  link.replaceWith(trigger);
  trigger.addEventListener('click', openPalette);
  // Closes a dialog and returns focus at once: the close event is queued and may come late.
  const shut = function (d) {
    d.close();
    (d === pal ? trigger : keysOpener || trigger).focus();
  };

  const input = make('input', '', null, {
    id: 'fb:palIn', type: 'search', role: 'combobox', autocomplete: 'off', spellcheck: 'false',
    'aria-label': 'Search the library', 'aria-autocomplete': 'list',
    'aria-expanded': 'true', 'aria-controls': 'fb:palList'
  });
  const palClose = make('button', 'btn ghost sm', 'Close', { type: 'button' });
  const list = make('ul', 'sr-list pal-list', null, { id: 'fb:palList', role: 'listbox', 'aria-label': 'Results' });
  pal.append(
    make('div', 'pal-in', [input, kbd('Esc'), palClose]),
    make('p', 'pal-note', null, { id: 'fb:palNote', role: 'status' }),
    list
  );
  // Shut by a backdrop press (down and up outside the box), Close or a followed row.
  const away = function (ev) {
    const r = pal.getBoundingClientRect();
    return ev.target === pal && !(ev.clientX >= r.left && ev.clientX < r.right &&
      ev.clientY >= r.top && ev.clientY < r.bottom);
  };
  let down = false;
  pal.addEventListener('pointerdown', function (ev) {
    down = away(ev);
  });
  pal.addEventListener('click', function (ev) {
    const li = closest(ev, '.pal-list li');
    const a = li && li.querySelector('a[href]');
    if (down && away(ev) || ev.target === palClose || a && closest(ev, 'a')) {
      shut(pal);
    } else if (a) {
      a.click();
    }
  });
  input.addEventListener('input', paint);
  input.addEventListener('keydown', function (ev) {
    if (ev.key === 'ArrowDown' || ev.key === 'ArrowUp') {
      ev.preventDefault();
      select(selected + (ev.key === 'ArrowDown' ? 1 : -1));
    } else if (ev.key === 'Enter') {
      ev.preventDefault();
      if (list.children[selected]) {
        list.children[selected].click();
      }
    }
  });

  const sw = make('button', 'switch', [make('i', '', null), 'Single-key shortcuts'],
    { type: 'button', 'aria-pressed': String(singleKeys) });
  const saved = make('p', 'note', failed, { role: 'status' });
  const close = make('button', 'btn ghost sm', 'Close', { type: 'button' });
  keys.append(make('div', 'dlg-h', [make('h2', '', 'Keyboard shortcuts'), close]), make('dl', 'keys', [
    make('dt', '', [kbd('/'), ' or ', kbd('Ctrl K')]), make('dd', '', 'Search'),
    make('dt', '', kbd('?')), make('dd', '', 'This list'),
    make('dt', '', kbd('Esc')), make('dd', '', 'Close a drawer, dialog or search')
  ]), sw, make('p', 'note', 'Ctrl K, Esc and every visible control still work.'), saved);
  close.addEventListener('click', function () {
    shut(keys);
  });
  sw.addEventListener('click', function () {
    singleKeys = !singleKeys;
    sw.setAttribute('aria-pressed', String(singleKeys));
    try {
      window.localStorage.setItem(KEYS_KEY, singleKeys ? 'on' : 'off');
      saved.textContent = '';
    } catch (_err) {
      saved.textContent = str('not_remembered');
    }
  });
  if (keysBtn) {
    keysBtn.hidden = false;
    keysBtn.addEventListener('click', openKeys);
  }

  document.addEventListener('keydown', function (ev) {
    const t = ev.target;
    const typing = t.isContentEditable || /^(INPUT|TEXTAREA|SELECT)$/.test(t.tagName);
    if ((ev.ctrlKey || ev.metaKey) && !ev.shiftKey && !ev.altKey && (ev.key === 'k' || ev.key === 'K')) {
      ev.preventDefault();
      openPalette();
    } else if (ev.key === 'Escape' && (pal.open || keys.open)) {
      ev.preventDefault();
      shut(pal.open ? pal : keys);
    } else if (singleKeys && !typing && !ev.ctrlKey && !ev.metaKey && !ev.altKey &&
      !pal.open && !keys.open && (ev.key === '/' || ev.key === '?')) {
      ev.preventDefault();
      (ev.key === '/' ? openPalette : openKeys)();
    }
  });
}

if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', initFrame);
} else {
  initFrame();
}
})();
