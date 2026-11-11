// Search over window.FB_INDEX per docs/reference/search.md. Integer scores.
(function () {
'use strict';
var SKELETON_DELAY_MS = 300;
var WORD = /[\p{Alphabetic}\p{N}]/uy;
var WORDS = /[\p{Alphabetic}\p{N}]+/gu;
// Lexer shapes 1-11; shape 10 drops commas.
var SHAPES = [/s-[0-9]{4}-v[0-9]+/y, /s-[0-9]{4}/y, /cl-[a-z]{3}-[0-9]{4}/y, /q-[0-9]{3}/y,
  /xc-[0-9]+/y, /cl-x[a-z]{2}-[0-9]{6,10}/y, /[a-z]{2}-[0-9]{6,10}-v[0-9]+/y,
  /[a-z]{2}-[0-9]{6,10}/y, /[0-9]{1,4}\.[0-9]{3}(?:-[0-9]+)?(?:\([a-z0-9]{1,4}\))*/y,
  /[0-9]+(?:\.[0-9]+|,[0-9]{3}(?![0-9]))*/y, /[\p{Alphabetic}\p{N}]+/uy];
var SP = '[\\t-\\r \\x85\\xa0\\u1680\\u2000-\\u200a\\u2028\\u2029\\u202f\\u205f\\u3000]+';
var TRIM = new RegExp('^' + SP + '|' + SP + '$', 'g');
var LINK = new Map([['page', 'p/'], ['clause', 'p/'], ['source', 's/'], ['text', 's/'],
  ['claim', 'claims/'], ['question', 'questions.html#'], ['conflict', 'conflicts.html#'],
  ['term', 'glossary.html#']]);
var CLAIMS_PER_PART = 500;
var me = document.currentScript;
var ROOT = ((me && me.getAttribute('src')) || '').replace(/assets\/search\.js$/, '');
var FIELDS = 'id kind title parent snippet status as_of effective_from verified_on'.split(' ');
var pending = null;

function norm(t) {
  return t.normalize('NFKC').toLowerCase().replace(/[\u2010-\u2015\u2212]/g, '-')
    .replace(/\u00ad/g, '');
}
function trim(t) { return t.replace(TRIM, ''); }
function wordAt(s, i) { WORD.lastIndex = i; return i < s.length && WORD.test(s); }

function lex(s) {
  var out = [], i = 0, k, m;
  while (i < s.length) {
    if (!wordAt(s, i)) { i += s.codePointAt(i) > 0xffff ? 2 : 1; continue; }
    for (k = 0; k < SHAPES.length; k += 1) {
      SHAPES[k].lastIndex = i;
      m = SHAPES[k].exec(s);
      if (m && !wordAt(s, i + m[0].length)) break;
    }
    out.push(k === 9 ? m[0].replace(/,/g, '') : m[0]);
    i += m[0].length;
  }
  return out;
}

// Query side: at most 12, no duplicates.
function tokens(text) {
  var out = [];
  lex(trim(norm(text))).forEach(function (t) {
    if (out.length < 12 && out.indexOf(t) < 0) out.push(t);
  });
  return out;
}

// First term not below t, in UTF-16 order.
function lowerBound(terms, t) {
  var lo = 0, hi = terms.length, mid;
  while (lo < hi) {
    mid = (lo + hi) >> 1;
    if (terms[mid] < t) lo = mid + 1;
    else hi = mid;
  }
  return lo;
}

function isExact(d, s) {
  var id = trim(norm(d[0])), key = trim(norm(d[9] || ''));
  var m = /^([\s\S]+)-v[0-9]+$/.exec(id);
  return s !== '' && (s === id || s === key ||
    (m !== null && (d[1] === 'source' || d[1] === 'record') && s === m[1]));
}

function hit(d, u, matched, score, exact) {
  var h = { unit: u, matched: matched, score: score, exact: exact };
  FIELDS.forEach(function (f, i) { h[f] = d[i]; });
  return h;
}

// Exact class, matched, sum, then unit.
function rank(ix, text, limit) {
  var s = trim(norm(text)), toks = tokens(text), terms = ix.terms, n = ix.docs.length;
  var matched = new Array(n).fill(0), sum = new Array(n).fill(0), hits = [];
  if (!toks.length) return { hits: hits, total: 0 };
  toks.forEach(function (tok, ti) {
    var score = new Array(n).fill(0), j = lowerBound(terms, tok), c = 0;
    function post(list, half) {
      for (var p = 0; p + 1 < list.length; p += 2) {
        var v = half ? Math.floor(list[p + 1] / 2) : list[p + 1];
        if (list[p] < n && v > score[list[p]]) score[list[p]] = v;
      }
    }
    if (terms[j] === tok) post(ix.post[j], false);
    if (ti === toks.length - 1 && tok.length >= 2) {
      for (; c < 64 && j < terms.length && terms[j].startsWith(tok); j += 1) {
        if (terms[j] !== tok) { post(ix.post[j], true); c += 1; }
      }
    }
    score.forEach(function (v, u) {
      if (v > 0) { matched[u] += 1; sum[u] += v; }
    });
  });
  ix.docs.forEach(function (d, u) {
    var exact = isExact(d, s);
    if (exact || matched[u]) hits.push(hit(d, u, matched[u], sum[u], exact));
  });
  hits.sort(function (a, b) {
    return (b.exact - a.exact) || (a.exact ? 0 : (b.matched - a.matched) ||
      (b.score - a.score)) || (a.unit - b.unit);
  });
  return { hits: limit == null ? hits : hits.slice(0, limit), total: hits.length };
}

function groups(kind) { return kind === 'page' || kind === 'clause'; }
function pageOf(id) { return id.split('#')[0]; }

// Page view: exact rows, a row per page, others.
function view(ix, hits) {
  var leads = new Map(), seen = new Set(), at = new Map();
  var v = { exact: [], pages: [], other: [], absorbed: 0 };
  ix.docs.forEach(function (d, u) {
    if (groups(d[1]) && d[0].indexOf('#') < 0) leads.set(d[0], u);
  });
  hits.forEach(function (h) {
    if (h.exact) v.exact.push(h);
    if (h.exact && groups(h.kind)) seen.add(pageOf(h.id));
  });
  hits.forEach(function (h) {
    var page = pageOf(h.id), u = leads.has(page) ? leads.get(page) : h.unit, row;
    if (h.exact) return;
    if (!groups(h.kind)) v.other.push(h);
    else if (seen.has(page)) v.absorbed += 1;
    else if (at.has(page)) v.pages[at.get(page)].matched_units += 1;
    else {
      row = hit(ix.docs[u], u, h.matched, h.score, false);
      row.id = page;
      row.to = h.id;
      row.snippet = h.snippet;
      row.has_lead = leads.has(page);
      row.matched_units = 1;
      at.set(page, v.pages.length);
      v.pages.push(row);
    }
  });
  return v;
}

// '' for a kind with no link form.
function link(kind, id) {
  var base = LINK.get(kind), i = id.indexOf('#');
  if (!base) return '';
  if (kind === 'claim') return ROOT + base + id.slice(3, 6) + '-' +
    (Math.ceil(id.slice(7) / CLAIMS_PER_PART) || 1) + '.html#' + id;
  if (base.slice(-1) === '#') return ROOT + base + id;
  if (i < 0 || kind === 'text') return ROOT + base + pageOf(id) + '.html';
  return ROOT + base + id.slice(0, i) + '.html' + id.slice(i);
}

function el(tag, cls, text) {
  var e = document.createElement(tag);
  e.className = cls;
  e.textContent = text;
  return e;
}

// Renders hits by textContent only.
function render(hits, text, list) {
  var words = tokens(text).join(' ').match(WORDS) || [];
  list.textContent = '';
  hits.forEach(function (h) {
    var li = el('li', 'sr-row', ''), href = link(h.kind, h.to || h.id);
    var head = el(href ? 'a' : 'span', 'sr-title', ''), str = h.title || h.id, from = 0;
    li.setAttribute('role', 'option');
    li.dataset.id = h.id;
    li.dataset.kind = h.kind;
    if (href) head.href = href;
    str.replace(WORDS, function (w, at) {
      if (words.some(function (q) { return norm(w).startsWith(q); })) {
        head.appendChild(el('span', '', str.slice(from, at)));
        head.appendChild(el('mark', '', w));
        from = at + w.length;
      }
    });
    head.appendChild(el('span', '', str.slice(from)));
    li.appendChild(head);
    if (h.parent) li.appendChild(el('div', 'sr-parent', h.parent));
    if (h.snippet) li.appendChild(el('p', 'sr-snip', h.snippet));
    li.appendChild(el('div', 'sr-meta', [h.kind, h.status, h.as_of && 'as of ' + h.as_of,
      h.effective_from && 'effective ' + h.effective_from,
      h.verified_on && 'verified ' + h.verified_on].filter(Boolean).join(' \u00b7 ')));
    list.appendChild(li);
  });
}

// Renders the first rows; returns the count.
function run(text, limit, list) {
  var ix = window.FB_INDEX, v = view(ix, rank(ix, text).hits);
  var all = v.exact.concat(v.pages, v.other);
  render(all.slice(0, limit), text, list);
  return all.length;
}

function usable(ix) {
  return ix && ix.v === 1 ? Promise.resolve(ix) : Promise.reject(ix ? 'version' : 'failed');
}

// Adds the index script once; a Skeleton only after SKELETON_DELAY_MS.
function load() {
  if (pending) return pending;
  if (window.FB_INDEX) return (pending = usable(window.FB_INDEX));
  pending = new Promise(function (resolve) {
    var box = $('out') || el('div', '', ''), bar = null;
    var script = document.createElement('script');
    var timer = window.setTimeout(function () {
      bar = el('div', 'skeleton', '');
      bar.setAttribute('aria-hidden', 'true');
      box.appendChild(bar);
    }, SKELETON_DELAY_MS);
    script.onload = script.onerror = function () {
      window.clearTimeout(timer);
      if (bar) bar.remove();
      box.removeAttribute('aria-busy');
      resolve(usable(window.FB_INDEX));
    };
    box.setAttribute('aria-busy', 'true');
    script.src = ROOT + 'assets/search-index.js';
    document.head.appendChild(script);
  });
  return pending;
}

window.FB_SEARCH = { load: load, rank: rank, render: render, run: run, tokens: tokens,
  view: view };

// The search page, search.html#q=.
var input = $('q');
if (!input) return;
function $(id) { return document.getElementById('fb:search-' + id); }
var list = $('results');

function show(which) {
  ['latest', 'more', 'none', 'failed', 'version'].forEach(function (k) {
    $(k).hidden = k !== which;
  });
}

function paint() {
  var text = input.value;
  load().then(function () {
    var total = tokens(text).length ? run(text, 50, list) : -1;
    $('total').textContent = total;
    if (total < 0) list.textContent = '';
    show(total < 0 ? 'latest' : total > 50 ? 'more' : total ? '' : 'none');
  }, function (why) {
    list.textContent = '';
    input.disabled = true;
    show(why);
  });
}

// A bad #q= escape is ''; any other hash is null.
function fromHash() {
  var h = location.hash;
  if (!/^(#q=|$)/.test(h)) return null;
  try { return decodeURIComponent(h.slice(3)); } catch (e) { return ''; }
}

$('blocked').hidden = true;
$('none').textContent = (window.FB_STRINGS || {}).abstention || '';
input.hidden = false;
input.addEventListener('focus', paint, { once: true });
input.addEventListener('input', function () { if (pending) paint(); });
input.addEventListener('keydown', function (ev) {
  if (ev.key === 'Enter') location.hash = 'q=' + encodeURIComponent(trim(input.value));
});
window.addEventListener('hashchange', function () {
  var q = fromHash();
  if (q !== null) { input.value = q; paint(); }
});
input.value = fromHash() || '';
if (input.value) paint();
}());
