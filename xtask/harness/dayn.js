// The Day-N cases of the browser harness (card views-and-day-n-2). dayn.html loads strings.js
// and this script, deferred. Each case rebuilds a synthetic frame, sets the stored start date and
// the clock (an injected Date), loads the site's day.js afresh and waits for it to run, so no case
// depends on today or on an earlier case. Outcomes are written into the DOM as in shortcuts.js:
// one <li data-case data-ok> per case, and the counts on #harness-summary.
(function () {
  'use strict';

  var out = document.getElementById('harness-results');
  var summary = document.getElementById('harness-summary');
  var counts = { run: 0, failed: 0 };
  var RealDate = window.Date;
  var storageDescriptor = Object.getOwnPropertyDescriptor(window, 'localStorage');
  var KEY = 'fieldbook.start';
  var runs = 0;

  // The load event is held back while cases are pending (see shortcuts.js).
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

  function $(id) {
    return document.getElementById(id);
  }

  function str(k) {
    return (window.FB_STRINGS || {})[k] || '';
  }

  function say(k, n) {
    return str(k).replace('N', n);
  }

  function key(k, target) {
    var ev = new KeyboardEvent('keydown', { key: k, bubbles: true, cancelable: true });
    target.dispatchEvent(ev);
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

  async function guarded(id, fn) {
    var problems = [];
    var check = function (ok, what) {
      if (!ok) {
        problems.push(what);
      }
    };
    try {
      await fn(check);
    } catch (e) {
      problems.push('threw ' + e);
    } finally {
      restore();
    }
    report(id, problems);
  }

  function restore() {
    window.Date = RealDate;
    Object.defineProperty(window, 'localStorage', storageDescriptor);
    try {
      localStorage.removeItem(KEY);
    } catch (_err) {}
  }

  // A Date whose "now" is the local noon of 2026-11-13; dates it is given keep their value.
  function clockAt(y, m, d) {
    var now = new RealDate(y, m - 1, d, 12).getTime();
    function FakeDate() {
      var args = [].slice.call(arguments);
      return args.length ? new (Function.prototype.bind.apply(RealDate, [null].concat(args)))() : new RealDate(now);
    }
    FakeDate.prototype = RealDate.prototype;
    FakeDate.parse = RealDate.parse;
    FakeDate.UTC = RealDate.UTC;
    FakeDate.now = function () { return now; };
    window.Date = FakeDate;
  }

  // Rebuilds the frame, applies the stored value and the clock, and runs day.js once.
  // `how.stored` is the value of fieldbook.start (absent when undefined); `how.throws` makes
  // every storage access throw.
  async function scenario(how) {
    $('stage').innerHTML =
      '<header class="topbar"><div class="topbar-end"><a class="build-date-link" href="about.html">Built</a></div></header>' +
      '<main id="fb:main" tabindex="-1"><section class="panel plan-panel"><div class="panel-h">' +
      '<h3 id="fb:plan-timeline-head">Timeline</h3><span class="meta"></span></div><div class="panel-b">' +
      '<div class="tl90" data-axis-max="90"><div class="tl-axis-row"></div></div></div></section>' +
      '<ul class="tu-list"><li class="tu-item" id="fb:startPrompt" hidden><button class="btn sm" type="button"></button></li></ul></main>';
    $('stage').querySelector('.meta').textContent = str('set_start_date_plan');
    $('stage').querySelector('#fb\\:startPrompt button').textContent = str('set_start_date');
    Array.prototype.forEach.call(document.querySelectorAll('dialog.dlg'), function (d) { d.remove(); });
    $('fb:toast').hidden = true;
    $('fb:toast').textContent = '';
    restore();
    if (how.stored !== undefined) {
      localStorage.setItem(KEY, how.stored);
    }
    if (how.throws) {
      Object.defineProperty(window, 'localStorage', {
        configurable: true,
        get: function () { throw new Error('storage blocked by the harness'); }
      });
    }
    clockAt(2026, 11, 13);
    await new Promise(function (resolve) {
      var s = document.createElement('script');
      runs += 1;
      s.onload = s.onerror = resolve;
      s.setAttribute('src', 'assets/day.js?run=' + runs);
      document.body.appendChild(s);
    });
  }

  function parts() {
    return {
      chip: $('fb:dayBtn'),
      dlg: document.querySelector('dialog.dlg'),
      field: $('fb:dayIn'),
      prompt: $('fb:startPrompt'),
      promptBtn: $('fb:startPrompt').querySelector('button'),
      head: document.querySelector('.plan-panel .meta'),
      line: document.querySelector('.tl-now'),
      main: document.querySelector('main')
    };
  }

  function stored() {
    try {
      return localStorage.getItem(KEY);
    } catch (_err) {
      return 'threw';
    }
  }

  async function cases() {
    await guarded('chip_hidden_without_a_date', async function (check) {
      await scenario({});
      var p = parts();
      check(p.chip && p.chip.hidden && p.chip.textContent === '', 'the chip is hidden and empty with no date');
      check(!p.prompt.hidden && !p.prompt.hasAttribute('data-day'), 'the start prompt shows, unmarked');
      check(p.head.textContent === str('set_start_date_plan'), 'the plan header keeps its unset text');
      check(p.line.hidden, 'no line is drawn');
    });

    await guarded('chip_shows_day_n_of_90', async function (check) {
      await scenario({ stored: '2026-11-01' });
      var p = parts();
      check(!p.chip.hidden && p.chip.textContent === say('day_n_of_90', 13), 'the chip reads ' + say('day_n_of_90', 13) + ', not ' + p.chip.textContent);
      check(p.chip.nextElementSibling && p.chip.nextElementSibling.classList.contains('build-date-link'), 'the chip sits before the build-date link');
      check(!p.chip.hasAttribute('title'), 'the chip has no tooltip');
    });

    await guarded('chip_opens_a_labelled_date_field', async function (check) {
      await scenario({});
      var p = parts();
      p.promptBtn.click();
      check(p.dlg.open, 'the dialog opens');
      check(p.field.type === 'date', 'the field is a date field');
      var label = $('fb:dayL');
      check(label && label.textContent === str('set_start_date') && label.htmlFor === 'fb:dayIn', 'the field is labelled ' + str('set_start_date'));
      check(p.dlg.getAttribute('aria-labelledby') === 'fb:dayL', 'the dialog is named by the label');
      check(document.activeElement === p.field, 'focus is in the field');
      check(p.dlg.querySelectorAll('button').length === 1 && p.dlg.querySelector('button').textContent === 'Close', 'one Close button');
    });

    await guarded('esc_returns_focus_to_the_chip_that_opened_it', async function (check) {
      await scenario({ stored: '2026-11-01' });
      var p = parts();
      p.chip.focus();
      p.chip.click();
      check(p.dlg.open, 'the dialog opens from the chip');
      key('Escape', p.field);
      check(!p.dlg.open, 'Esc closes the dialog');
      check(document.activeElement === p.chip, 'focus returns to the chip');
      check(stored() === '2026-11-01', 'Esc saves nothing');
    });

    await guarded('esc_returns_focus_to_the_start_prompt_that_opened_it', async function (check) {
      await scenario({});
      var p = parts();
      p.promptBtn.focus();
      p.promptBtn.click();
      key('Escape', p.field);
      check(!p.dlg.open, 'Esc closes the dialog');
      check(document.activeElement === p.promptBtn, 'focus returns to the start prompt, not the hidden chip');
    });

    await guarded('changing_the_date_saves_it_shows_the_chip_and_marks_the_prompt', async function (check) {
      await scenario({});
      var p = parts();
      p.promptBtn.click();
      p.field.value = '2026-11-12';
      key('Enter', p.field);
      check(stored() === '2026-11-12', 'the date is stored');
      check(!p.chip.hidden && p.chip.textContent === say('day_n_of_90', 2), 'the chip reads ' + say('day_n_of_90', 2));
      check(!p.prompt.hidden && p.prompt.hasAttribute('data-day'), 'the row stays shown with its data attribute');
      check(!p.dlg.open, 'the dialog closes');
    });

    await guarded('invalid_stored_date_is_ignored', async function (check) {
      var bad = ['2026-02-30', 'not a date', '2026-1-5', '2027-01-01'];
      for (var i = 0; i < bad.length; i += 1) {
        await scenario({ stored: bad[i] });
        var p = parts();
        check(p.chip.hidden && p.chip.textContent === '', bad[i] + ' shows no chip');
        check(p.head.textContent === str('set_start_date_plan') && p.line.hidden, bad[i] + ' leaves the plan unmarked');
      }
    });

    await guarded('typing_a_date_saves_nothing_until_enter', async function (check) {
      await scenario({});
      var p = parts();
      p.promptBtn.click();
      key('0', p.field);
      p.field.value = '0002-11-13';
      p.field.dispatchEvent(new Event('change', { bubbles: true }));
      check(stored() === null, 'a change after a key press stores nothing');
      check(p.dlg.open, 'the dialog stays open');
      p.field.value = '2026-11-13';
      key('Enter', p.field);
      check(stored() === '2026-11-13', 'Enter stores the date');
      check(!p.dlg.open, 'Enter closes the dialog');
    });

    await guarded('a_date_picker_change_saves_at_once', async function (check) {
      await scenario({});
      var p = parts();
      p.promptBtn.click();
      p.field.value = '2026-11-10';
      p.field.dispatchEvent(new Event('change', { bubbles: true }));
      check(stored() === '2026-11-10', 'the picker change stores the date');
      check(!p.dlg.open, 'the dialog closes');
    });

    await guarded('close_and_the_backdrop_save_nothing', async function (check) {
      await scenario({});
      var p = parts();
      p.promptBtn.click();
      p.field.value = '2026-11-10';
      p.dlg.querySelector('button').click();
      check(!p.dlg.open && stored() === null, 'Close saves nothing');
      p.promptBtn.click();
      p.field.value = '2026-11-10';
      p.dlg.dispatchEvent(new MouseEvent('click', { bubbles: true }));
      check(!p.dlg.open && stored() === null, 'a click on the backdrop saves nothing');
      check(p.chip.hidden, 'the chip stays hidden');
    });

    await guarded('saving_returns_focus_to_the_opener_or_its_fallback', async function (check) {
      await scenario({});
      var p = parts();
      p.promptBtn.focus();
      p.promptBtn.click();
      p.field.value = '2026-11-10';
      key('Enter', p.field);
      check(document.activeElement === p.promptBtn, 'focus returns to the start prompt that opened it');
      await scenario({ stored: '2026-11-10' });
      p = parts();
      p.chip.focus();
      p.chip.click();
      p.field.value = '';
      key('Enter', p.field);
      check(stored() === null && p.chip.hidden, 'an emptied field removes the date and hides the chip');
      check(document.activeElement === p.main, 'focus falls back to main when the opener is hidden');
    });

    await guarded('plan_header_and_line_follow_the_date', async function (check) {
      await scenario({ stored: '2026-11-01' });
      var p = parts();
      check(p.head.textContent === say('day_n_today', 13), 'the header reads ' + say('day_n_today', 13) + ', not ' + p.head.textContent);
      check(!p.line.hidden && Math.abs(parseFloat(p.line.style.left) - 13 / 90 * 100) < 0.01, 'the line sits at 13 of 90, not ' + p.line.style.left);
      check(p.line.parentNode.classList.contains('tl-axis-row'), 'the line is drawn over the track');
    });

    await guarded('plan_line_is_absent_after_day_90_and_the_header_says_day_n_of_90', async function (check) {
      await scenario({ stored: '2026-06-01' });
      var p = parts();
      check(p.line.hidden, 'no line after Day 90');
      check(p.head.textContent === say('day_n_of_90', 166), 'the header reads ' + say('day_n_of_90', 166) + ', not ' + p.head.textContent);
      check(p.chip.textContent === say('day_n_of_90', 166), 'the chip shows the real N');
    });

    await guarded('storage_throwing_keeps_the_page_working_and_shows_the_not_remembered_toast', async function (check) {
      await scenario({ throws: true });
      var p = parts();
      check(p.chip.hidden, 'the page loads with no date');
      p.promptBtn.click();
      p.field.value = '2026-11-10';
      key('Enter', p.field);
      var toast = $('fb:toast');
      check(!toast.hidden && toast.textContent === str('not_remembered'), 'the toast says ' + str('not_remembered'));
      check(!p.chip.hidden && p.chip.textContent === say('day_n_of_90', 4), 'the chip keeps its day for this visit');
      check(p.chip.textContent !== str('not_remembered'), 'the notice is never chip text');
    });
  }

  hold();
  document.addEventListener('DOMContentLoaded', async function () {
    await cases();
    finished = true;
    summary.setAttribute('data-run', String(counts.run));
    summary.setAttribute('data-failed', String(counts.failed));
    summary.setAttribute('data-done', 'true');
    summary.textContent = counts.run + ' cases, ' + counts.failed + ' failed';
  });
}());
