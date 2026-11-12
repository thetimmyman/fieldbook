// The search cases of the browser harness (card search-js). Generated pages load the golden
// wrappers (window.FB_GOLDEN), the indexes Rust built from the ranked files
// (window.FB_GOLDEN_INDEX), search.js and then this script. Every outcome is written into the
// DOM: one <li data-case="<id>" data-ok="true|false"> per case in #harness-results, and the
// counts on #harness-summary. The page's <body data-harness> picks the cases:
//   goldens   every query-side tokenize case, every ranked case, every page-view case,
//             ordered_hits_render_without_reranking, index_text_goes_in_as_text, the hash
//             cases and the timing of 10 lookups (on assets/search-index.js when the page
//             loads the site's index before search.js, else on the largest ranked golden);
//   skeleton  skeleton_waits_300ms;
//   version   unsupported_index_version_keeps_the_id_list_line.
(function () {
  'use strict';

  var S = window.FB_SEARCH;
  var out = document.getElementById('harness-results');
  var summary = document.getElementById('harness-summary');
  var counts = { run: 0, failed: 0 };
  // The site's index, when the goldens page loads one for the timing.
  var SITE = window.FB_INDEX;

  function report(id, problems) {
    var li = document.createElement('li');
    counts.run += 1;
    if (problems.length) {
      counts.failed += 1;
    }
    li.setAttribute('data-case', id);
    li.setAttribute('data-ok', problems.length ? 'false' : 'true');
    li.textContent = id + (problems.length ? ': ' + problems.join('; ') : ': ok');
    out.appendChild(li);
  }

  // Runs one case; an exception fails that case by name instead of stopping the run.
  function guarded(id, fn) {
    var problems;
    try {
      problems = fn();
    } catch (e) {
      problems = ['threw ' + e];
    }
    report(id, problems);
  }

  // --dump-dom writes the DOM once the load event has fired, and a script element added before
  // then delays that event until it loads. So while cases are pending the page keeps one such
  // script (hold.js, empty) in flight, and finish() ends the chain. It is added to <body>, since
  // skeleton_waits_300ms records what goes into <head>. After 30 s the chain stops and the
  // page is written unfinished, which fails it by name.
  var finished = false;
  var holdUntil = Date.now() + 30000;
  function hold() {
    if (finished || Date.now() > holdUntil) {
      return;
    }
    var s = document.createElement('script');
    s.onload = s.onerror = function () {
      s.remove();
      hold();
    };
    s.setAttribute('src', 'hold.js');
    document.body.appendChild(s);
  }

  function finish(extra) {
    finished = true;
    summary.setAttribute('data-run', String(counts.run));
    summary.setAttribute('data-failed', String(counts.failed));
    summary.setAttribute('data-done', 'true');
    summary.textContent = counts.run + ' cases, ' + counts.failed + ' failed' + (extra || '');
  }

  function same(a, b) {
    return JSON.stringify(a) === JSON.stringify(b);
  }

  function differ(name, expected, got) {
    return name + ': expected ' + JSON.stringify(expected) + ', got ' + JSON.stringify(got);
  }

  function goldens(kind) {
    var all = window.FB_GOLDEN || {};
    return Object.keys(all).sort().filter(function (key) {
      return all[key].kind === kind;
    }).map(function (key) {
      return { key: key, file: all[key], index: (window.FB_GOLDEN_INDEX || {})[key] };
    });
  }

  function posting(index, unit, token) {
    var u = index.docs.findIndex(function (d) { return d[0] === unit; });
    var t = index.terms.indexOf(token);
    var list = t < 0 ? [] : index.post[t];
    for (var p = 0; p + 1 < list.length; p += 2) {
      if (list[p] === u) {
        return list[p + 1];
      }
    }
    return null;
  }

  function tokenizeCases() {
    goldens('tokenize').forEach(function (g) {
      g.file.cases.filter(function (c) { return c.side === 'query'; }).forEach(function (c) {
        guarded(c.id, function () {
          var got = S.tokens(c.input);
          return same(got, c.tokens) ? [] : [differ('tokens', c.tokens, got)];
        });
      });
    });
  }

  function rankedCases() {
    goldens('ranked').forEach(function (g) {
      g.file.cases.forEach(function (c) {
        guarded(c.id, function () { return rankedCase(g, c); });
      });
    });
  }

  function rankedCase(g, c) {
    var problems = [];
    var r = S.rank(g.index, c.query, 10);
    var ids = r.hits.map(function (h) { return h.id; });
    var scores = r.hits.map(function (h) { return h.score; });
    if (!g.index) {
      problems.push('no index for ' + g.key);
    }
    if (!same(ids, c.expect)) {
      problems.push(differ('list', c.expect, ids));
    }
    if (c.scores && !same(scores, c.scores)) {
      problems.push(differ('scores', c.scores, scores));
    }
    (c.impacts || []).forEach(function (p) {
      var got = posting(g.index, p.unit, p.token);
      if (got !== p.impact) {
        problems.push(differ('impact ' + p.unit + ' ' + p.token, p.impact, got));
      }
    });
    return problems;
  }

  function ids(list) {
    return list.map(function (h) { return h.id; });
  }

  // What search.js shows of `list` (ids in view order): the first `limit`, with places held for term rows, at most `cap`, kept in the original order.
  function heldRows(index, list, limit, cap) {
    var kind = {};
    index.docs.forEach(function (d) { kind[d[0]] = d[1]; });
    var terms = list.filter(function (id) { return kind[id] === 'term'; }).slice(0, cap);
    var rest = list.filter(function (id) { return terms.indexOf(id) < 0; }).slice(0, limit - terms.length);
    return list.filter(function (id) { return terms.indexOf(id) >= 0 || rest.indexOf(id) >= 0; });
  }

  function pageViewCases() {
    goldens('page-view').forEach(function (g) {
      g.file.cases.forEach(function (c) {
        guarded(c.id, function () { return pageViewCase(g, c); });
      });
    });
  }

  // The view against the golden, then what search.js does with it: run() renders the first
  // 50 of the exact, page and other rows with places held for term rows (at most 5), and
  // returns their count. The CLI's shown_pages and shown_other cut is Rust's to test; the
  // site has its own rule.
  function pageViewCase(g, c) {
    var fields = ['kind', 'title', 'parent', 'status', 'as_of', 'effective_from', 'verified_on'];
    var problems = [];
    var r = S.rank(g.index, c.query);
    var v = S.view(g.index, r.hits);
    var pages = v.pages.map(function (row) {
      var p = { page: row.id, has_lead: row.has_lead };
      fields.forEach(function (f) { p[f] = row[f]; });
      p.best_unit = row.to;
      p.snippet = row.snippet;
      p.matched_units = row.matched_units;
      return p;
    });
    var accounted = v.exact.length + v.absorbed + v.other.length +
      v.pages.reduce(function (n, p) { return n + p.matched_units; }, 0);
    if (!same(ids(r.hits), c.hits)) {
      problems.push(differ('hits', c.hits, ids(r.hits)));
    }
    if (!same(ids(v.exact), c.exact)) {
      problems.push(differ('exact', c.exact, ids(v.exact)));
    }
    if (!same(pages, c.pages)) {
      problems.push(differ('pages', c.pages, pages));
    }
    if (!same(ids(v.other), c.other)) {
      problems.push(differ('other', c.other, ids(v.other)));
    }
    if (v.absorbed !== c.absorbed) {
      problems.push(differ('absorbed', c.absorbed, v.absorbed));
    }
    if (accounted !== r.total) {
      problems.push(differ('accounting', r.total, accounted));
    }
    var rows = c.exact.concat(c.pages.map(function (p) { return p.page; }), c.other);
    var list = document.createElement('ul');
    var saved = window.FB_INDEX;
    var count;
    window.FB_INDEX = g.index;
    try {
      count = S.run(c.query, 50, list);
    } finally {
      window.FB_INDEX = saved;
    }
    var shown = Array.prototype.map.call(list.children, function (li) {
      return li.getAttribute('data-id');
    });
    if (count !== rows.length) {
      problems.push(differ('run count', rows.length, count));
    }
    if (!same(shown, heldRows(g.index, rows, 50, 5))) {
      problems.push(differ('rows shown', heldRows(g.index, rows, 50, 5), shown));
    }
    return problems;
  }

  // rank answers from the index it is passed, and render keeps the order it is given, reads no
  // index, and links only a kind with a link form. The ordinary index (seed-ranked) is in
  // window.FB_INDEX throughout, so a rank that reads it instead of its argument fails here:
  // 'withhold' then comes back in seed order, and 'S-0042' brings seed-only units.
  function orderedHitsRenderWithoutReranking() {
    var problems = [];
    var g = goldens('ranked').filter(function (x) { return /seed-ranked/.test(x.key); })[0];
    var permitted = goldens('ranked').filter(function (x) { return /worked-example/.test(x.key); })[0];
    var want = permitted.file.cases.filter(function (c) { return c.id === 'rk-02'; })[0];
    var allowed = permitted.index.docs.map(function (d) { return d[0]; });
    var list = document.createElement('ul');
    var record = { id: 'CM-000012-v2', kind: 'record', title: 'Synthetic record', parent: 'quote',
      snippet: 'A record with no page yet.', status: 'active', as_of: '2026-01-01',
      effective_from: '', verified_on: '' };
    var saved = window.FB_INDEX;
    window.FB_INDEX = g.index;
    try {
      var ranked = S.rank(permitted.index, want.query, 10).hits;
      if (!same(ids(ranked), want.expect)) {
        problems.push(differ('permitted list', want.expect, ids(ranked)));
      }
      if (!same(ranked.map(function (h) { return h.score; }), want.scores)) {
        problems.push(differ('permitted scores', want.scores,
          ranked.map(function (h) { return h.score; })));
      }
      ids(S.rank(permitted.index, 'S-0042', 10).hits).forEach(function (id) {
        if (allowed.indexOf(id) < 0) {
          problems.push('ordinary unit exposed: ' + id);
        }
      });
      var hits = ranked.slice().reverse();
      S.render(hits.concat([record]), want.query, list);
      var rows = Array.prototype.slice.call(list.children);
      var got = rows.map(function (li) { return li.getAttribute('data-id'); });
      var order = ids(hits).concat([record.id]);
      if (!same(got, order)) {
        problems.push(differ('order', order, got));
      }
      var last = rows[rows.length - 1];
      if (!last || last.querySelector('a') || last.querySelector('[href]')) {
        problems.push('a record without a link form rendered a link');
      }
      if (!last || last.textContent.indexOf('Synthetic record') < 0 ||
          last.textContent.indexOf('A record with no page yet.') < 0) {
        problems.push('a record without a link lost its text');
      }
      rows.slice(0, -1).forEach(function (li) {
        var a = li.querySelector('a');
        if (!a || !a.getAttribute('href')) {
          problems.push(li.getAttribute('data-id') + ' has no link');
        }
        if (li.getAttribute('role') !== 'option') {
          problems.push(li.getAttribute('data-id') + ' is not an option');
        }
      });
    } catch (e) {
      problems.push('threw ' + e);
    } finally {
      window.FB_INDEX = saved;
    }
    report('ordered_hits_render_without_reranking', problems);
  }

  // Index text is text: markup in a title, parent or snippet shows literally and builds nothing.
  function indexTextGoesInAsText() {
    guarded('index_text_goes_in_as_text', function () {
      var problems = [];
      var list = document.createElement('ul');
      var title = '<img src=x onerror=window.__hit=1>';
      delete window.__hit;
      S.render([{ id: 'cmp-x', kind: 'page', title: title, parent: '<i>p</i>',
        snippet: '<b>x</b>', status: 'draft', as_of: '', effective_from: '',
        verified_on: '' }], 'cmp', list);
      document.body.appendChild(list);
      if (list.querySelector('img, b, i, script')) {
        problems.push('markup in index text built an element');
      }
      [title, '<i>p</i>', '<b>x</b>'].forEach(function (t) {
        if (list.textContent.indexOf(t) < 0) {
          problems.push('text lost: ' + t);
        }
      });
      list.remove();
      if (window.__hit !== undefined) {
        problems.push('an index string ran');
      }
      return problems;
    });
  }

  // Term rows render with aliases and metadata, and held places keep terms in the cut.
  function termRowsAndHeldPlaces() {
    function markTexts(node) {
      return Array.prototype.map.call(node.querySelectorAll('mark'), function (m) {
        return m.textContent;
      });
    }

    // A term row renders its link, alias line, snippet, metadata and query marks.
    guarded('term_row_renders_its_link_alias_line_and_meta', function () {
      var problems = [];
      var list = document.createElement('ul');
      var hit = { id: 'wrap_rate', kind: 'term', title: 'wrap rate',
        parent: 'fully burdened rate|FBR', snippet: 'A direct labour rate.',
        status: 'general_practice', as_of: '', effective_from: '', verified_on: '' };
      S.render([hit], 'burdened rate', list);
      document.body.appendChild(list);
      try {
        var li = list.querySelector('li');
        if (!li || li.getAttribute('data-kind') !== 'term') {
          problems.push('the row li has no data-kind term');
        }
        if (!li || li.getAttribute('role') !== 'option') {
          problems.push('the row li is not an option');
        }
        var titleA = li ? li.querySelector('a.sr-title') : null;
        var href = titleA ? (titleA.getAttribute('href') || '') : '';
        if (!titleA || !/(^|\/)glossary\.html#wrap_rate$/.test(href)) {
          problems.push('title link does not end in glossary.html#wrap_rate');
        }
        var parent = li ? li.querySelector('.sr-parent') : null;
        if (!parent) {
          problems.push('no .sr-parent rendered');
        } else {
          if (parent.textContent !== 'Also called: fully burdened rate') {
            problems.push(differ('sr-parent text', 'Also called: fully burdened rate', parent.textContent));
          }
          var pMarks = markTexts(parent);
          if (!same(pMarks, ['burdened', 'rate'])) {
            problems.push(differ('sr-parent marks', ['burdened', 'rate'], pMarks));
          }
        }
        var snip = li ? li.querySelector('.sr-snip') : null;
        if (!snip || snip.textContent !== 'A direct labour rate.') {
          problems.push(differ('snippet text', 'A direct labour rate.', snip ? snip.textContent : null));
        }
        var meta = li ? li.querySelector('.sr-meta') : null;
        if (!meta || meta.textContent !== 'glossary term \u00b7 general practice') {
          problems.push(differ('meta text', 'glossary term \u00b7 general practice', meta ? meta.textContent : null));
        }
        var tMarks = titleA ? markTexts(titleA) : [];
        if (!same(tMarks, ['rate'])) {
          problems.push(differ('title marks', ['rate'], tMarks));
        }

        S.render([hit], 'wrap', list);
        if (list.querySelector('.sr-parent')) {
          problems.push('sr-parent rendered when no alias matches query');
        }

        hit.parent = '';
        S.render([hit], 'rate', list);
        if (list.querySelector('.sr-parent')) {
          problems.push('sr-parent rendered when parent is empty');
        }

        S.render([{ id: 't_markup', kind: 'term', title: 'term', parent: '<i>x</i>|y',
          snippet: '', status: '', as_of: '', effective_from: '',
          verified_on: '' }], 'x', list);
        if (list.querySelector('i')) {
          problems.push('markup in alias built an i element');
        }
      } finally {
        list.remove();
      }
      return problems;
    });

    // Hold keeps up to cap term rows within limit without mutating the input rows.
    guarded('held_places_keep_term_rows_in_the_cut', function () {
      var problems = [];
      var rows = [];
      var i;
      for (i = 1; i <= 60; i += 1) {
        rows.push({
          id: 'p-' + (i < 10 ? '0' : '') + i, kind: 'page', title: 'P', parent: '',
          snippet: '', status: '', as_of: '', effective_from: '', verified_on: ''
        });
      }
      for (i = 1; i <= 7; i += 1) {
        rows.push({
          id: 't_' + i, kind: 'term', title: 'T', parent: '',
          snippet: '', status: '', as_of: '', effective_from: '', verified_on: ''
        });
      }

      var got1 = ids(S.hold(rows, 50, 5));
      var want1 = ids(rows.slice(0, 45)).concat(ids(rows.slice(60, 65)));
      if (!same(got1, want1)) {
        problems.push(differ('hold 50 5', want1, got1));
      }

      var got2 = ids(S.hold(rows.slice(0, 62), 50, 5));
      var want2 = ids(rows.slice(0, 48)).concat(ids(rows.slice(60, 62)));
      if (!same(got2, want2)) {
        problems.push(differ('hold 50 5 with 2 terms', want2, got2));
      }

      var slice30 = rows.slice(0, 30);
      var got3 = ids(S.hold(slice30, 50, 5));
      var want3 = ids(slice30);
      if (!same(got3, want3)) {
        problems.push(differ('hold 30 under limit', want3, got3));
      }

      var got4 = ids(S.hold(rows, 8, 2));
      var want4 = ids(rows.slice(0, 6)).concat(ids(rows.slice(60, 62)));
      if (!same(got4, want4)) {
        problems.push(differ('hold 8 2', want4, got4));
      }

      if (rows.length !== 67) {
        problems.push(differ('rows length', 67, rows.length));
      }

      return problems;
    });
  }

  // The page's hash. Each step sets the hash and checks after the page has painted.
  function hashCases(done) {
    var input = $('q');
    var steps = [];
    var at = 0;
    function rows() {
      return Array.prototype.map.call($('results').children, function (li) {
        return li.getAttribute('data-id');
      });
    }
    function next() {
      if (at >= steps.length) {
        window.removeEventListener('hashchange', settle);
        done();
        return;
      }
      var step = steps[at];
      at += 1;
      try {
        step.act();
      } catch (e) {
        report(step.id, ['threw ' + e]);
        next();
        return;
      }
    }
    var current = null;
    function settle() {
      if (current) {
        var step = current;
        current = null;
        window.setTimeout(function () { check(step); }, 0);
      }
    }
    function check(step) {
      guarded(step.id, step.check);
      next();
    }
    function onHash(id, hash, check) {
      return {
        id: id, check: check,
        act: function () { current = this; window.location.hash = hash; }
      };
    }
    var typed;
    var shown;
    window.addEventListener('hashchange', settle);
    window.FB_INDEX = goldens('ranked').filter(function (x) { return /seed-ranked/.test(x.key); })[0].index;
    steps.push(onHash('hash_query_is_decoded', 'q=' + encodeURIComponent('cmp-cdi#qa'), function () {
      shown = rows();
      var problems = [];
      if (input.value !== 'cmp-cdi#qa') {
        problems.push(differ('input', 'cmp-cdi#qa', input.value));
      }
      if (!same(shown.slice(0, 1), ['cmp-cdi#qa'])) {
        problems.push(differ('first row', ['cmp-cdi#qa'], shown));
      }
      return problems;
    }));
    steps.push(onHash('a_non_query_hash_keeps_the_results', 'fb:main', function () {
      var problems = [];
      if (input.value !== 'cmp-cdi#qa') {
        problems.push(differ('input', 'cmp-cdi#qa', input.value));
      }
      if (!same(rows(), shown)) {
        problems.push(differ('rows', shown, rows()));
      }
      return problems;
    }));
    steps.push(onHash('an_undecodable_hash_is_an_empty_query', 'q=%E0%A4%A', function () {
      var problems = [];
      if (input.value !== '') {
        problems.push(differ('input', '', input.value));
      }
      if (rows().length || $('latest').hidden) {
        problems.push('results show for an empty query');
      }
      return problems;
    }));
    // A leading U+FEFF is White_Space to neither trim: the query keeps it, and so must #q=.
    steps.push({
      id: 'enter_keeps_the_typed_results',
      act: function () {
        var list = document.createElement('ul');
        current = this;
        input.value = '\ufeffcmp-cdi#qa';
        S.run(input.value, 50, list);
        typed = Array.prototype.map.call(list.children, function (li) {
          return li.getAttribute('data-id');
        });
        input.dispatchEvent(new KeyboardEvent('keydown', { key: 'Enter' }));
      },
      check: function () {
        var problems = [];
        var want = '#q=' + encodeURIComponent('\ufeffcmp-cdi#qa');
        if (window.location.hash !== want) {
          problems.push(differ('hash', want, window.location.hash));
        }
        if (!same(rows(), typed)) {
          problems.push(differ('rows after Enter', typed, rows()));
        }
        return problems;
      }
    });
    next();
  }

  // Advisory: 10 lookups, each a rank and the page view, on the site's index when the page
  // loads one (assets/search-index.js from just site) and otherwise on the largest ranked
  // golden index; prints the p95 and never fails.
  function timing(site) {
    var name = 'the site index';
    var ix = site;
    if (!ix || !ix.docs) {
      var g = goldens('ranked').sort(function (a, b) {
        return b.index.docs.length - a.index.docs.length;
      })[0];
      ix = g.index;
      name = g.key;
    }
    var queries = ['withhold', 'S-0042', '7012', '252.204', 'payment withhold', 'ice', 'cmp-cdi#qa',
      'subcontractors', 'cl-cmp-0011', 're'];
    var times = queries.map(function (q) {
      var t0 = window.performance.now();
      S.view(ix, S.rank(ix, q).hits);
      return window.performance.now() - t0;
    }).sort(function (a, b) { return a - b; });
    var p95 = times[Math.ceil(times.length * 0.95) - 1];
    summary.setAttribute('data-p95-ms', p95.toFixed(3));
    return ' (advisory: p95 ' + p95.toFixed(3) + ' ms over 10 lookups on ' + name + ', ' +
      ix.docs.length + ' units)';
  }

  function $(id) {
    return document.getElementById('fb:search-' + id);
  }

  // Replaces setTimeout, clearTimeout and the insertion of the index script with recorders.
  // Part one lets the timer run before the index arrives; part two runs a fresh copy of
  // search.js and lets the index arrive first.
  function skeletonWaits300ms(done) {
    var problems = [];
    var timers = [];
    var cleared = [];
    var scripts = [];
    var real = {
      set: window.setTimeout, clear: window.clearTimeout, append: document.head.appendChild
    };
    var bars = function () { return document.querySelectorAll('.skeleton').length; };
    var empty = function () { return { v: 1, docs: [], terms: [], post: [] }; };
    function record() {
      window.setTimeout = function (fn, ms) {
        timers.push({ fn: fn, ms: ms });
        return timers.length;
      };
      window.clearTimeout = function (id) { cleared.push(id); };
      document.head.appendChild = function (el) {
        scripts.push(el);
        return el;
      };
    }
    function restore() {
      window.setTimeout = real.set;
      window.clearTimeout = real.clear;
      document.head.appendChild = real.append;
    }
    function indexScript(el) {
      return /(^|\/)assets\/search-index\.js$/.test(el.getAttribute('src') || '');
    }

    delete window.FB_INDEX;
    record();
    $('q').dispatchEvent(new Event('focus'));
    if (timers.length !== 1 || timers[0].ms !== 300) {
      problems.push(differ('timers after focus', [300], timers.map(function (t) { return t.ms; })));
    }
    if (bars() !== 0) {
      problems.push('a skeleton shows before the timer runs');
    }
    if (scripts.length !== 1 || !indexScript(scripts[0])) {
      problems.push('the index script was not added once');
    }
    if (timers.length) {
      timers[0].fn();
    }
    if (bars() !== 1) {
      problems.push(differ('skeletons after the timer', 1, bars()));
    }
    window.FB_INDEX = empty();
    if (scripts.length) {
      scripts[0].onload();
    }
    if (bars() !== 0) {
      problems.push(differ('skeletons after onload', 0, bars()));
    }
    restore();

    // A fresh copy of search.js has no pending load.
    var copy = document.createElement('script');
    copy.onload = function () {
      var first = timers.length;
      delete window.FB_INDEX;
      record();
      window.FB_SEARCH.load();
      if (timers.length !== first + 1 || timers[first].ms !== 300) {
        problems.push('the second load set no 300 ms timer');
      }
      window.FB_INDEX = empty();
      if (scripts.length !== 2 || !indexScript(scripts[1])) {
        problems.push('the second load added no index script');
      } else {
        scripts[1].onload();
      }
      if (cleared.indexOf(first + 1) < 0) {
        problems.push('onload before the timer did not clear it');
      }
      if (bars() !== 0) {
        problems.push('a skeleton shows when the index loads first');
      }
      restore();
      report('skeleton_waits_300ms', problems);
      done();
    };
    copy.setAttribute('src', document.querySelector('script[data-search]').getAttribute('src'));
    document.head.appendChild(copy);
  }

  function unsupportedIndexVersion() {
    var input = $('q');
    input.dispatchEvent(new Event('focus'));
    S.load().then(function () {
      report('unsupported_index_version_keeps_the_id_list_line', ['an index with v 2 was used']);
      finish();
    }, function () {
      var problems = [];
      var line = $('version');
      var link = line && line.querySelector('a[href]');
      // The page's own rejection handler runs first; check on the next turn.
      window.setTimeout(function () {
        if (!line || line.hidden) {
          problems.push('the index-version line is hidden');
        } else if (line.textContent.replace(/\s+/g, ' ').trim() !==
          'Search is unavailable for this index version. Use the ID list.') {
          problems.push(differ('line', 'Search is unavailable for this index version. Use the ID list.', line.textContent));
        }
        if (!link || !/(^|\/)ids\.html$/.test(link.getAttribute('href'))) {
          problems.push('the line has no link to ids.html');
        }
        if ($('results').children.length) {
          problems.push('ranked results are shown');
        }
        if (!$('blocked').hidden) {
          problems.push('the scripts-blocked line shows');
        }
        report('unsupported_index_version_keeps_the_id_list_line', problems);
        finish();
      }, 0);
    });
  }

  hold();
  var mode = document.body.getAttribute('data-harness');
  if (mode === 'skeleton') {
    skeletonWaits300ms(function () { finish(); });
  } else if (mode === 'version') {
    unsupportedIndexVersion();
  } else {
    tokenizeCases();
    rankedCases();
    pageViewCases();
    orderedHitsRenderWithoutReranking();
    indexTextGoesInAsText();
    termRowsAndHeldPlaces();
    hashCases(function () { finish(timing(SITE)); });
  }
}());
