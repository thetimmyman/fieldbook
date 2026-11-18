(function () {
'use strict';

// DayChip, start-date Dialog and plan today line (HANDOFF.md 5, 11); loaded after app.js.
const KEY = 'fieldbook.start';

function init() {
  const byId = id => document.getElementById('fb:' + id);
  const str = k => (window.FB_STRINGS || {})[k] || '';
  const $ = (e, q) => e.querySelector(q);
  const make = (tag, cls, kids, attrs) => {
    const e = document.createElement(tag);
    e.className = cls;
    e.append(...[].concat(kids || []));
    Object.keys(attrs || {}).forEach(k => e.setAttribute(k, attrs[k]));
    return e;
  };
  const prompt = byId('startPrompt');
  const bar = $(document, '.topbar-end');
  const chip = make('button', 'chip solid day-chip', null, { id: 'fb:dayBtn', type: 'button', 'aria-haspopup': 'dialog' });
  const field = make('input', '', null, { id: 'fb:dayIn', type: 'date' });
  const close = make('button', 'btn ghost sm', 'Close', { type: 'button' });
  const label = make('label', '', str('set_start_date'), { id: 'fb:dayL', for: 'fb:dayIn' });
  const dlg = make('dialog', 'dlg', make('div', 'dlg-b', [label, field, close]), { 'aria-labelledby': 'fb:dayL' });
  const plans = [...document.querySelectorAll('.plan-panel')].map(p => {
    const line = make('span', 'tl-now', null, { 'aria-hidden': 'true' });
    const head = $(p, '.meta'), track = $(p, '.tl90');
    $(track, '.tl-axis-row').appendChild(line);
    return { head, line, max: track.dataset.axisMax, unset: head.textContent };
  });
  let start, opener, typed, day;
  try {
    start = localStorage.getItem(KEY);
  } catch (_err) {}

  function toast(message) {
    const t = byId('toast');
    byId('sr').textContent = t.textContent = message;
    t.hidden = false;
    t.classList.add('show');
    setTimeout(() => (t.classList.remove('show'), t.hidden = true), 3200);
  }

  function localIso() {
    const d = new Date();
    return new Date(d - d.getTimezoneOffset() * 6e4).toISOString().slice(0, 10);
  }

  // N: whole days from a real stored date to today, plus 1; NaN for anything else.
  function paint() {
    const t = /^\d{4}-\d\d-\d\d$/.test(start) ? Date.parse(start) : NaN;
    const say = k => str(k).replace('N', day);
    day = t === t && new Date(t).toISOString().slice(0, 10) === start ? (Date.parse(localIso()) - t) / 864e5 + 1 : NaN;
    chip.hidden = !(day > 0);
    chip.textContent = day > 0 ? say('day_n_of_90') : '';
    prompt && (prompt.hidden = false, prompt.toggleAttribute('data-day', day > 0));
    plans.forEach(p => {
      p.head.textContent = day > 0 ? say(day > 90 ? 'day_n_of_90' : 'day_n_today') : p.unset;
      p.line.hidden = !(day > 0 && day < 91);
      p.line.style.left = Math.min(day / p.max, 1) * 100 + '%';
    });
  }

  function open(ev) {
    opener = ev.currentTarget;
    typed = false;
    field.value = day === day ? start : '';
    dlg.showModal();
    field.focus();
  }

  // Focus returns to the opener, else the chip, else main: the first that shows.
  function shut() {
    dlg.close();
    [opener, chip, $(document, 'main'), document.body].find(e => e && e.getClientRects().length).focus();
  }

  function save() {
    start = field.value;
    try {
      start ? localStorage.setItem(KEY, start) : localStorage.removeItem(KEY);
    } catch (_err) {
      toast(str('not_remembered'));
    }
    paint();
    shut();
  }

  bar && bar.insertBefore(chip, $(bar, ':scope > .build-date-link'));
  document.body.append(dlg);
  chip.onclick = open;
  prompt && ($(prompt, 'button').onclick = open);
  dlg.onclick = ev => (ev.target === dlg || ev.target === close) && shut();
  // Chromium sends change at the first digit typed, so after a key press only Enter saves.
  dlg.onkeydown = ev => {
    if (ev.key === 'Escape') {
      ev.preventDefault();
      shut();
    } else if (ev.target === field) {
      ev.key !== 'Enter' ? typed = true : field.validity.badInput || save();
    }
  };
  field.onchange = () => typed || save();
  paint();
}

init();
})();
