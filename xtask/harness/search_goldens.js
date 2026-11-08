// The search cases of the browser harness (card search-js). Generated pages load the golden
// wrappers (window.FB_GOLDEN), the indexes Rust built from the ranked files
// (window.FB_GOLDEN_INDEX), search.js and then this script. Every outcome is written into the
// DOM: one <li data-case="<id>" data-ok="true|false"> per case in #harness-results, and the
// counts on #harness-summary. The page's <body data-harness> picks the cases:
//   goldens   every query-side tokenize case, every ranked case, every page-view case,
//             ordered_hits_render_without_reranking and the timing of 10 lookups;
//   skeleton  skeleton_waits_300ms;
//   version   unsupported_index_version_keeps_the_id_list_line.
(function () {
  'use strict';

  var S = window.FB_SEARCH;
  var out = document.getElementById('harness-results');
  var summary = document.getElementById('harness-summary');
  var counts = { run: 0, failed: 0 };

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

  function finish(extra) {
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
        var got = S.tokens(c.input);
        report(c.id, same(got, c.tokens) ? [] : [differ('tokens', c.tokens, got)]);
      });
    });
  }

  function rankedCases() {
    goldens('ranked').forEach(function (g) {
      g.file.cases.forEach(function (c) {
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
        report(c.id, problems);
      });
    });
  }

  function pageViewCases() {
    var fields = ['kind', 'title', 'parent', 'status', 'as_of', 'effective_from', 'verified_on'];
    goldens('page-view').forEach(function (g) {
      g.file.cases.forEach(function (c) {
        var problems = [];
        var r = S.rank(g.index, c.query);
        var v = S.view(g.index, r.hits);
        var ids = function (list) { return list.map(function (h) { return h.id; }); };
        var pages = v.pages.map(function (row) {
          var p = { page: row.id, has_lead: row.has_lead };
          fields.forEach(function (f) { p[f] = row[f]; });
          p.best_unit = row.to;
          p.snippet = row.snippet;
          p.matched_units = row.matched_units;
          return p;
        });
        var shownPages = Math.min(10, v.pages.length);
        var shownOther = Math.min(v.pages.length ? 5 : 10, v.other.length);
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
        if (shownPages !== c.shown_pages || shownOther !== c.shown_other) {
          problems.push(differ('shown', [c.shown_pages, c.shown_other], [shownPages, shownOther]));
        }
        if (accounted !== r.total) {
          problems.push(differ('accounting', r.total, accounted));
        }
        report(c.id, problems);
      });
    });
  }

  // render keeps the order it is given, reads no index, and links only a kind with a link form.
  function orderedHitsRenderWithoutReranking() {
    var problems = [];
    var g = goldens('ranked').filter(function (x) { return /seed-ranked/.test(x.key); })[0];
    var permitted = goldens('ranked').filter(function (x) { return /worked-example/.test(x.key); })[0];
    var list = document.createElement('ul');
    var ordinary = window.FB_INDEX;
    var hits = S.rank(permitted.index, 'withhold', 10).hits.slice().reverse();
    var record = { id: 'CM-000012-v2', kind: 'record', title: 'Synthetic record', parent: 'quote',
      snippet: 'A record with no page yet.', status: 'active', as_of: '2026-01-01',
      effective_from: '', verified_on: '' };
    var rows;
    window.FB_INDEX = g.index;
    S.render(hits.concat([record]), 'withhold', list);
    rows = Array.prototype.slice.call(list.children);
    var got = rows.map(function (li) { return li.getAttribute('data-id'); });
    var want = hits.map(function (h) { return h.id; }).concat([record.id]);
    if (!same(got, want)) {
      problems.push(differ('order', want, got));
    }
    got.forEach(function (id) {
      var inPermitted = permitted.index.docs.some(function (d) { return d[0] === id; });
      if (!inPermitted && id !== record.id) {
        problems.push('a unit outside the permitted index was rendered: ' + id);
      }
    });
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
    window.FB_INDEX = ordinary;
    report('ordered_hits_render_without_reranking', problems);
  }

  // Advisory: 10 lookups on the largest ranked index; prints the p95 and never fails.
  function timing() {
    var g = goldens('ranked').sort(function (a, b) {
      return b.index.docs.length - a.index.docs.length;
    })[0];
    var queries = ['withhold', 'S-0042', '7012', '252.204', 'payment withhold', 'ice', 'cmp-cdi#qa',
      'subcontractors', 'cl-cmp-0011', 'review'];
    var times = queries.map(function (q) {
      var t0 = window.performance.now();
      S.rank(g.index, q, 50);
      return window.performance.now() - t0;
    }).sort(function (a, b) { return a - b; });
    var p95 = times[Math.ceil(times.length * 0.95) - 1];
    summary.setAttribute('data-p95-ms', p95.toFixed(3));
    return ' (advisory: p95 ' + p95.toFixed(3) + ' ms over 10 lookups)';
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
    finish(timing());
  }
}());
