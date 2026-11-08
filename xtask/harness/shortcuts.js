// The shortcut and palette cases of the browser harness (card search-palette). shortcuts.html
// loads this script first, then strings.js, search.js and app.js, all deferred; this script
// installs a small synthetic index as window.FB_INDEX (and, for storage-throws, storage that
// throws on every access) before app.js starts, and runs the cases at DOMContentLoaded, after
// every deferred script. The harness opens the page with --dump-dom and no virtual time, which
// writes the DOM once the load event has fired, so this script holds that event back (hold.js,
// as in search_goldens.js) until every case has reported.
// Every outcome is written into the DOM: one <li data-case="<id>" data-ok="true|false"> per
// case in #harness-results, and the counts on #harness-summary. <body data-harness> picks the
// cases:
//   shortcuts       every shortcut case and every palette case;
//   storage-throws  storage_throwing_page_still_works.
// Events are synthetic, so they never type a character or move focus by Tab: a case reads
// defaultPrevented, the dialogs' open state and document.activeElement instead.
(function () {
  'use strict';

  var MODE = document.body.getAttribute('data-harness');
  var out = document.getElementById('harness-results');
  var summary = document.getElementById('harness-summary');
  var counts = { run: 0, failed: 0 };

  // --dump-dom writes the DOM once the load event has fired, and a script element added before
  // then delays that event until it loads. So while cases are pending the page keeps one such
  // script (hold.js, empty) in flight, and setting `finished` ends the chain. After 30 s it stops
  // and the page is written unfinished, which fails it by name.
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
  // Links the palette followed; the capture below keeps the harness page in place.
  var followed = [];

  // [id, kind, title, parent, snippet, status, as_of], in unit order.
  var DOCS = [
    ['ab-alpha', 'page', 'Alpha budget', '', 'Alpha snippet', 'draft', '2025-01-01'],
    ['ab-alpha#scope', 'page', 'Scope', 'ab-alpha', 'Scope snippet', '', ''],
    ['ab-beta', 'page', 'Beta abacus', '', '', '', '2025-03-01'],
    ['ca-01', 'page', 'Plain one', '', '', '', ''],
    ['ca-02', 'page', 'Plain two', '', '', '', ''],
    ['ca-03', 'page', 'Plain three', '', '', '', ''],
    ['ca-04', 'page', 'Plain four', '', '', '', ''],
    ['ca-05', 'page', 'Plain five', '', '', '', ''],
    ['cd-gamma', 'page', 'Gamma overhead', '', 'Overhead rates', '', '2025-02-01'],
    ['rx-0001', 'record', 'Rogue record', '', 'A record with no page', '', ''],
    ['zz-abc', 'page', 'Abacus rules', '', '', '', ''],
    ['zz-cas1', 'page', 'Cascade one', '', '', '', ''],
    ['zz-cas2', 'page', 'Cascade two', '', '', '', ''],
    ['zz-cas3', 'page', 'Cascade three', '', '', '', ''],
    ['zz-cas4', 'page', 'Cascade four', '', '', '', ''],
    ['zz-cas5', 'page', 'Cascade five', '', '', '', ''],
    ['zz-cas6', 'page', 'Cascade six', '', '', '', ''],
    ['zz-note', 'page', 'A note on rates', '', '', '', '']
  ];

  // Title words only, impact 10: plain ASCII words lex the same in search.js.
  function syntheticIndex() {
    var posts = new Map();
    DOCS.forEach(function (d, u) {
      (d[2].toLowerCase().match(/[a-z0-9]+/g) || []).forEach(function (t) {
        if (!posts.has(t)) {
          posts.set(t, []);
        }
        if (posts.get(t)[posts.get(t).length - 2] !== u) {
          posts.get(t).push(u, 10);
        }
      });
    });
    var terms = Array.from(posts.keys()).sort();
    return {
      v: 1,
      terms: terms,
      post: terms.map(function (t) { return posts.get(t); }),
      docs: DOCS.map(function (d) { return d.concat(['', '', '']); })
    };
  }

  window.FB_INDEX = syntheticIndex();

  if (MODE === 'storage-throws') {
    ['localStorage', 'sessionStorage'].forEach(function (name) {
      Object.defineProperty(window, name, {
        configurable: true,
        get: function () { throw new Error('storage blocked by the harness'); }
      });
    });
  }

  document.addEventListener('click', function (ev) {
    var a = ev.target && ev.target.closest ? ev.target.closest('a[href]') : null;
    if (a) {
      ev.preventDefault();
      followed.push(a.getAttribute('href'));
    }
  }, true);

  function $(id) {
    return document.getElementById(id);
  }

  function tick() {
    return new Promise(function (resolve) { window.setTimeout(resolve, 0); });
  }

  // Dispatches a keydown on `target` (the focused element by default); returns the event.
  function key(k, opts, target) {
    var init = { key: k, bubbles: true, cancelable: true };
    Object.keys(opts || {}).forEach(function (o) { init[o] = opts[o]; });
    var ev = new KeyboardEvent('keydown', init);
    (target || document.activeElement || document.body).dispatchEvent(ev);
    return ev;
  }

  function report(id, problems) {
    var li = document.createElement('li');
    counts.run += 1;
    if (problems.length) {
      counts.failed += 1;
    }
    li.setAttribute('data-case', id);
    li.setAttribute('data-ok', problems.length ? 'false' : 'true');
    li.textContent = id + ' (' + id.replace(/_/g, ' ') + ')' +
      (problems.length ? ': ' + problems.join('; ') : ': ok');
    out.appendChild(li);
  }

  // Runs one case after closing any open dialog; a throw fails that case by name.
  async function guarded(id, fn) {
    var problems = [];
    var check = function (ok, what) {
      if (!ok) {
        problems.push(what);
      }
    };
    try {
      await closeAll();
      $('harness-field').blur();
      await fn(check);
    } catch (e) {
      problems.push('threw ' + e);
    }
    report(id, problems);
  }

  async function closeAll() {
    ['fb:palette', 'fb:keys'].forEach(function (id) {
      if ($(id).open) {
        $(id).close();
      }
    });
    await tick();
  }

  function pal() { return $('fb:palette'); }
  function keys() { return $('fb:keys'); }
  function trigger() { return $('fb:searchBtn'); }
  function inside(dlg) { return dlg.open && dlg.contains(document.activeElement); }

  // Opens the palette with Ctrl K, types `text` and waits for the rows.
  async function search(text) {
    key('k', { ctrlKey: true }, document.body);
    var input = $('fb:palIn');
    input.value = text;
    input.dispatchEvent(new Event('input', { bubbles: true }));
    await tick();
    await tick();
    var opts = Array.prototype.slice.call($('fb:palList').children);
    return {
      opts: opts,
      ids: opts.filter(function (li) { return li.hasAttribute('data-id'); })
        .map(function (li) { return li.getAttribute('data-id'); }),
      last: opts[opts.length - 1],
      note: $('fb:palNote').textContent
    };
  }

  function same(a, b) {
    return JSON.stringify(a) === JSON.stringify(b);
  }

  function differ(name, expected, got) {
    return name + ': expected ' + JSON.stringify(expected) + ', got ' + JSON.stringify(got);
  }

  function finalSearchRow(r, text, check) {
    var a = r.last && r.last.querySelector('a');
    check(r.last && r.last.getAttribute('role') === 'option' && !r.last.hasAttribute('data-id'),
      'the last row is the Search row');
    check(a && a.getAttribute('href') === 'search.html#q=' + encodeURIComponent(text),
      differ('Search href', 'search.html#q=' + encodeURIComponent(text), a && a.getAttribute('href')));
    check(a && a.textContent === 'Search for ' + text,
      differ('Search text', 'Search for ' + text, a && a.textContent));
  }

  async function shortcutCases() {
    await guarded('trigger_is_a_button_showing_ctrl_k', async function (check) {
      var t = trigger();
      check(t.tagName === 'BUTTON', 'the trigger is ' + t.tagName);
      check(t.getAttribute('aria-label') === 'Search the library (Control K)', 'its name');
      check(t.querySelector('kbd') && t.querySelector('kbd').textContent === 'Ctrl K', 'Ctrl K');
      t.click();
      check(pal().open && document.activeElement === $('fb:palIn'), 'a click opens the palette');
    });

    await guarded('ctrl_k_opens', async function (check) {
      var ev = key('k', { ctrlKey: true }, document.body);
      check(pal().open, 'the palette is open');
      check(document.activeElement === $('fb:palIn'), 'focus is in the input');
      check(ev.defaultPrevented, 'the browser does not see Ctrl K');
      await closeAll();
      ['shiftKey', 'altKey'].forEach(function (mod) {
        var opts = { ctrlKey: true };
        opts[mod] = true;
        var other = key('K', opts, document.body);
        check(!pal().open && !other.defaultPrevented, 'Ctrl K with ' + mod + ' is left to the browser');
      });
    });

    await guarded('slash_opens', async function (check) {
      key('/', {}, document.body);
      check(pal().open, 'the palette is open');
      check(document.activeElement === $('fb:palIn'), 'focus is in the input');
    });

    await guarded('slash_in_a_text_field_types', async function (check) {
      var field = $('harness-field');
      field.focus();
      var ev = key('/', {}, field);
      check(!pal().open, 'the palette stays closed');
      check(!ev.defaultPrevented, 'the character is left to the field');
      check(document.activeElement === field, 'focus stays in the field');
    });

    await guarded('esc_closes_and_returns_focus', async function (check) {
      key('k', { ctrlKey: true }, document.body);
      key('Escape', {}, $('fb:palIn'));
      // The dialog's close event is queued, so wait for it (up to half a second), not one tick.
      for (var i = 0; i < 20 && document.activeElement !== trigger(); i++) {
        await new Promise(function (resolve) { window.setTimeout(resolve, 25); });
      }
      check(!pal().open, 'the palette is closed');
      check(document.activeElement === trigger(), 'focus is back on the trigger, not ' +
        (document.activeElement && (document.activeElement.id || document.activeElement.tagName)));
    });

    await guarded('question_mark_opens_the_shortcuts_dialog', async function (check) {
      key('?', { shiftKey: true }, document.body);
      check(keys().open, 'the dialog is open');
      check(inside(keys()), 'focus is inside the dialog');
      var lines = Array.prototype.map.call(keys().querySelectorAll('dd'), function (dd) {
        return dd.textContent;
      });
      var spec = ['Search', 'This list', 'Close a drawer, dialog or search'];
      check(same(lines, spec), differ('listed keys', spec, lines));
      key('Escape');
      await tick();
      check(!keys().open, 'Esc closes it');
      var link = $('fb:keysBtn');
      check(!link.hidden, 'the Keyboard shortcuts link is shown');
      link.focus();
      link.click();
      check(keys().open, 'the link opens it');
      key('Escape');
      await tick();
      check(document.activeElement === link, 'Esc returns focus to the link');
    });

    await guarded('modal_dialogs_contain_focus', async function (check) {
      key('k', { ctrlKey: true }, document.body);
      check(pal().matches(':modal'), 'the palette is modal');
      check(inside(pal()), 'the palette contains the focus');
      await closeAll();
      key('?', {}, document.body);
      check(keys().matches(':modal'), 'the shortcuts dialog is modal');
      check(inside(keys()), 'the shortcuts dialog contains the focus');
    });

    await guarded('switch_off_stops_single_keys_but_not_ctrl_k', async function (check) {
      var written = {};
      key('?', {}, document.body);
      var sw = keys().querySelector('.switch');
      check(sw.getAttribute('aria-pressed') === 'true', 'on by default');
      sw.click();
      check(sw.getAttribute('aria-pressed') === 'false', 'the switch turns off');
      try {
        written.off = window.localStorage.getItem('fieldbook.singleKeys');
      } catch (e) {
        written.off = 'threw';
      }
      await closeAll();
      check(!key('/', {}, document.body).defaultPrevented && !pal().open, '/ does nothing');
      check(!key('?', {}, document.body).defaultPrevented && !keys().open, '? does nothing');
      key('k', { ctrlKey: true }, document.body);
      check(pal().open, 'Ctrl K still opens the palette');
      await closeAll();
      $('fb:keysBtn').click();
      sw.click();
      check(sw.getAttribute('aria-pressed') === 'true', 'the switch turns on again');
      try {
        written.on = window.localStorage.getItem('fieldbook.singleKeys');
        written.keys = Object.keys(window.localStorage).filter(function (k) {
          return k.indexOf('fieldbook.') === 0;
        });
        window.localStorage.removeItem('fieldbook.singleKeys');
      } catch (e) {
        written.on = 'threw';
      }
      check(written.off === 'off' && written.on === 'on', differ('stored', 'off then on', written));
      check(same(written.keys, ['fieldbook.singleKeys']), differ('keys written', ['fieldbook.singleKeys'], written.keys));
    });
  }

  async function paletteCases() {
    await guarded('palette_empty_input_lists_latest_pages', async function (check) {
      var r = await search('');
      check(r.note === 'Latest pages', differ('note', 'Latest pages', r.note));
      check(same(r.ids, ['ab-beta', 'cd-gamma', 'ab-alpha']), differ('rows', ['ab-beta', 'cd-gamma', 'ab-alpha'], r.ids));
      check(r.opts.length === r.ids.length, 'no Search row for an empty input');
    });

    // The title-only row (zz-note) is reachable only through ranked fallback.
    await guarded('palette_one_character_prefix_first', async function (check) {
      var r = await search('a');
      check(same(r.ids, ['ab-alpha', 'ab-beta', 'zz-note']), differ('rows', ['ab-alpha', 'ab-beta', 'zz-note'], r.ids));
      check(r.ids.indexOf('ab-alpha#scope') < 0, 'no section unit by prefix');
      check(r.opts[0].getAttribute('aria-selected') === 'true', 'the first row is selected');
      check($('fb:palIn').getAttribute('aria-activedescendant') === r.opts[0].id, 'activedescendant');
      check($('fb:palIn').getAttribute('aria-autocomplete') === 'list', 'aria-autocomplete is list');
      check(!$('fb:palIn').hasAttribute('placeholder'), 'no placeholder the spec does not write');
    });

    await guarded('palette_duplicates_appear_once', async function (check) {
      var r = await search('ab');
      check(same(r.ids, ['ab-alpha', 'ab-beta', 'zz-abc']), differ('rows', ['ab-alpha', 'ab-beta', 'zz-abc'], r.ids));
    });

    await guarded('palette_caps_at_eight', async function (check) {
      var r = await search('ca');
      var expect = ['ca-01', 'ca-02', 'ca-03', 'ca-04', 'ca-05', 'zz-cas1', 'zz-cas2', 'zz-cas3'];
      check(same(r.ids, expect), differ('rows', expect, r.ids));
      check(r.opts.length === 9, 'eight rows and the Search row, got ' + r.opts.length);
      finalSearchRow(r, 'ca', check);
    });

    await guarded('palette_absent_link_enter_does_not_navigate', async function (check) {
      var r = await search('rogue');
      followed.length = 0;
      check(same(r.ids, ['rx-0001']), differ('rows', ['rx-0001'], r.ids));
      check(!r.opts[0].querySelector('a'), 'the record row has no link');
      check(r.opts[0].textContent.indexOf('Rogue') >= 0, 'its text is shown');
      key('Enter', {}, $('fb:palIn'));
      check(followed.length === 0, differ('followed', [], followed));
      check(pal().open, 'the palette stays open');
    });

    await guarded('palette_linked_result_enter_navigates', async function (check) {
      var r = await search('gamma');
      var mark = r.opts[0] && r.opts[0].querySelector('.sr-title mark');
      followed.length = 0;
      check(same(r.ids, ['cd-gamma']), differ('rows', ['cd-gamma'], r.ids));
      check(mark && mark.textContent === 'Gamma', 'the query word is highlighted');
      key('ArrowDown', {}, $('fb:palIn'));
      check(r.opts[1].getAttribute('aria-selected') === 'true', 'Down selects the Search row');
      key('ArrowUp', {}, $('fb:palIn'));
      key('Enter', {}, $('fb:palIn'));
      check(same(followed, ['p/cd-gamma.html']), differ('followed', ['p/cd-gamma.html'], followed));
      check(!pal().open, 'following a row closes the palette');
      check(document.activeElement === trigger(), 'focus is back on the trigger');
    });

    // A row is one target: a click anywhere on it follows its link and closes the palette.
    await guarded('palette_row_click_follows_its_link', async function (check) {
      var r = await search('gamma');
      followed.length = 0;
      r.opts[0].querySelector('.sr-meta').click();
      check(same(followed, ['p/cd-gamma.html']), differ('followed', ['p/cd-gamma.html'], followed));
      check(!pal().open, 'the palette closes');
      r = await search('rogue');
      followed.length = 0;
      r.opts[0].click();
      check(followed.length === 0, differ('followed', [], followed));
      check(pal().open, 'a row without a link keeps the palette open');
    });

    // Pointer and touch close the palette too: the backdrop, and Close below 768px.
    await guarded('palette_backdrop_and_close_button_close', async function (check) {
      await search('gamma');
      pal().dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
      check(!pal().open, 'a click on the backdrop closes the palette');
      check(document.activeElement === trigger(), 'focus is back on the trigger');
      await search('gamma');
      var close = pal().querySelector('.pal-in button');
      check(close && close.textContent === 'Close', 'the palette has a Close button');
      if (close) {
        close.click();
      }
      check(!pal().open, 'Close closes the palette');
      check(document.activeElement === trigger(), 'Close returns focus to the trigger');
      await search('gamma');
      $('fb:palIn').click();
      check(pal().open, 'a click inside the palette keeps it open');
    });

    await guarded('palette_final_search_row', async function (check) {
      var r = await search('zzzq nothing');
      var phrase = (window.FB_STRINGS || {}).abstention;
      check(r.ids.length === 0, differ('rows', [], r.ids));
      check(r.note === phrase, differ('note', phrase, r.note));
      finalSearchRow(r, 'zzzq nothing', check);
      followed.length = 0;
      key('Enter', {}, $('fb:palIn'));
      check(same(followed, ['search.html#q=zzzq%20nothing']), differ('followed', ['search.html#q=zzzq%20nothing'], followed));
      check(!pal().open, 'the Search row closes the palette');
    });
  }

  async function storageCases() {
    await guarded('storage_throwing_page_still_works', async function (check) {
      key('/', {}, document.body);
      check(pal().open, '/ opens the palette');
      await closeAll();
      key('?', {}, document.body);
      var phrase = (window.FB_STRINGS || {}).not_remembered;
      check(phrase && keys().textContent.indexOf(phrase) >= 0, 'the dialog says ' + phrase + ' before any toggle');
      var sw = keys().querySelector('.switch');
      sw.click();
      check(sw.getAttribute('aria-pressed') === 'false', 'the switch still turns off');
      check(phrase && keys().textContent.indexOf(phrase) >= 0, 'the dialog says ' + phrase);
      await closeAll();
      check(!key('/', {}, document.body).defaultPrevented && !pal().open, 'the choice lasts the session');
      key('k', { ctrlKey: true }, document.body);
      check(pal().open, 'Ctrl K opens the palette');
      var r = await search('gamma');
      check(same(r.ids, ['cd-gamma']), differ('rows', ['cd-gamma'], r.ids));
    });
  }

  hold();
  document.addEventListener('DOMContentLoaded', async function () {
    if (MODE === 'storage-throws') {
      await storageCases();
    } else {
      await shortcutCases();
      await paletteCases();
    }
    await closeAll();
    finished = true;
    summary.setAttribute('data-run', String(counts.run));
    summary.setAttribute('data-failed', String(counts.failed));
    summary.setAttribute('data-done', 'true');
    summary.textContent = counts.run + ' cases, ' + counts.failed + ' failed';
  });
}());
