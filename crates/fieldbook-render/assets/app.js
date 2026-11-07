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

  function showToast(message) {
    if (!message) {
      return;
    }
    const toast = document.getElementById('fb:toast');
    const sr = document.getElementById('fb:sr');
    if (sr) {
      sr.textContent = message;
    }
    if (!toast) {
      return;
    }
    toast.textContent = message;
    toast.hidden = false;
    toast.classList.add('show');
    if (toastTimer !== null) {
      window.clearTimeout(toastTimer);
    }
    toastTimer = window.setTimeout(function () {
      toast.classList.remove('show');
      toast.hidden = true;
    }, 3200);
  }

  function systemPrefersDark() {
    return Boolean(
      window.matchMedia &&
        window.matchMedia('(prefers-color-scheme: dark)').matches
    );
  }

  function effectiveTheme() {
    const attr = document.documentElement.getAttribute('data-theme');
    if (attr === 'dark' || attr === 'light') {
      return attr;
    }
    if (sessionTheme === 'dark' || sessionTheme === 'light') {
      return sessionTheme;
    }
    return systemPrefersDark() ? 'dark' : 'light';
  }

  function syncThemeButton() {
    const btn = document.getElementById('fb:themeBtn');
    if (!btn) {
      return;
    }
    const isDark = effectiveTheme() === 'dark';
    btn.setAttribute('aria-pressed', isDark ? 'true' : 'false');
  }

  function applyTheme(theme) {
    sessionTheme = theme;
    if (theme === 'dark' || theme === 'light') {
      document.documentElement.setAttribute('data-theme', theme);
    } else {
      document.documentElement.removeAttribute('data-theme');
    }
    syncThemeButton();
  }

  function initTheme() {
    let stored = null;
    try {
      stored = window.localStorage.getItem(THEME_KEY);
    } catch (_err) {
      stored = null;
    }
    if (stored === 'dark' || stored === 'light') {
      applyTheme(stored);
    } else {
      syncThemeButton();
    }
  }

  function toggleTheme() {
    const next = effectiveTheme() === 'dark' ? 'light' : 'dark';
    applyTheme(next);
    try {
      window.localStorage.setItem(THEME_KEY, next);
    } catch (_err) {
      const msg =
        window.FB_STRINGS && window.FB_STRINGS.not_remembered
          ? window.FB_STRINGS.not_remembered
          : '';
      showToast(msg);
    }
  }

  function selectSidebarSection(secKey) {
    const groups = document.querySelectorAll('.sidebar .nav-group[data-sec]');
    for (let i = 0; i < groups.length; i += 1) {
      const group = groups[i];
      group.hidden = group.getAttribute('data-sec') !== secKey;
    }
    const picks = (document.getElementById('fb:secSwitch') || document.createElement('div')).querySelectorAll('[data-secpick]');
    for (let i = 0; i < picks.length; i += 1) {
      const btn = picks[i];
      const active = btn.getAttribute('data-secpick') === secKey;
      btn.setAttribute('aria-pressed', active ? 'true' : 'false');
    }
  }

  function isOverlayOpen() {
    const sidebar = document.getElementById('fb:sidebar');
    return Boolean(sidebar && sidebar.classList.contains('open'));
  }

  function openSidebar() {
    const sidebar = document.getElementById('fb:sidebar');
    const scrim = document.getElementById('fb:scrim');
    const menuBtn = document.getElementById('fb:menuBtn');
    if (!sidebar) {
      return;
    }
    lastTrigger = document.activeElement;
    sidebar.classList.add('open');
    if (scrim) {
      scrim.hidden = false;
      scrim.classList.add('open');
    }
    if (menuBtn) {
      menuBtn.setAttribute('aria-expanded', 'true');
    }
    const firstFocusable = sidebar.querySelector(
      'button:not([hidden]), a[href]:not([hidden]), [tabindex]:not([tabindex="-1"])'
    );
    if (firstFocusable && typeof firstFocusable.focus === 'function') {
      firstFocusable.focus();
    }
  }

  function closeSidebar(restoreFocus) {
    const sidebar = document.getElementById('fb:sidebar');
    const scrim = document.getElementById('fb:scrim');
    const menuBtn = document.getElementById('fb:menuBtn');
    if (!sidebar || !sidebar.classList.contains('open')) {
      return;
    }
    sidebar.classList.remove('open');
    if (scrim) {
      scrim.classList.remove('open');
      scrim.hidden = true;
    }
    if (menuBtn) {
      menuBtn.setAttribute('aria-expanded', 'false');
    }
    if (restoreFocus) {
      const target = lastTrigger || menuBtn;
      if (target && typeof target.focus === 'function') {
        target.focus();
      }
    }
  }

  function initFrame() {
    initTheme();

    const themeBtn = document.getElementById('fb:themeBtn');
    if (themeBtn) {
      themeBtn.addEventListener('click', toggleTheme);
    }

    const menuBtn = document.getElementById('fb:menuBtn');
    if (menuBtn) {
      menuBtn.addEventListener('click', function () {
        if (isOverlayOpen()) {
          closeSidebar(true);
        } else {
          openSidebar();
        }
      });
    }

    const closeBtn = document.getElementById('fb:sidebarCloseBtn');
    if (closeBtn) {
      closeBtn.addEventListener('click', function () {
        closeSidebar(true);
      });
    }

    const scrim = document.getElementById('fb:scrim');
    if (scrim) {
      scrim.addEventListener('click', function () {
        closeSidebar(true);
      });
    }

    const secSwitch = document.getElementById('fb:secSwitch');
    if (secSwitch) {
      secSwitch.addEventListener('click', function (ev) {
        const btn =
          ev.target && typeof ev.target.closest === 'function'
            ? ev.target.closest('[data-secpick]')
            : null;
        if (!btn) {
          return;
        }
        const sec = btn.getAttribute('data-secpick');
        if (sec) {
          selectSidebarSection(sec);
        }
      });
    }

    document.addEventListener('keydown', function (ev) {
      if (ev.key === 'Escape' && isOverlayOpen()) {
        ev.preventDefault();
        closeSidebar(true);
      }
    });

    if (window.matchMedia) {
      const mq = window.matchMedia('(min-width: 960px)');
      const onChange = function (e) {
        if (e.matches && isOverlayOpen()) {
          closeSidebar(false);
        }
      };
      if (typeof mq.addEventListener === 'function') {
        mq.addEventListener('change', onChange);
      }
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
  }

  function localIsoDate() {
    const now = new Date();
    const y = String(now.getFullYear());
    const m = String(now.getMonth() + 1).padStart(2, '0');
    const d = String(now.getDate()).padStart(2, '0');
    return y + '-' + m + '-' + d;
  }

  function applyWording(mode) {
    const reader = document.getElementById('fb:reader') || document.body;
    const isPlain = mode === 'plain';
    if (reader) {
      reader.classList.toggle('hide-precise', isPlain);
    }
    const buttons = (document.getElementById('fb:wordingSeg') || document.createElement('div')).querySelectorAll('[data-wording]');
    for (let i = 0; i < buttons.length; i += 1) {
      const btn = buttons[i];
      const active = btn.getAttribute('data-wording') === (isPlain ? 'plain' : 'both');
      btn.setAttribute('aria-pressed', active ? 'true' : 'false');
    }
  }

  function showPopoverFor(el) {
    const pop = document.getElementById('fb:popover');
    if (!pop || !el) {
      return;
    }
    let title = el.getAttribute('data-pv-title') || '';
    let body = el.getAttribute('data-pv-body') || '';
    const pv = el.getAttribute('data-pv') || '';
    if (!body && pv.indexOf('cite:') === 0) {
      const num = pv.slice(5);
      const row = document.getElementById('cite-' + num);
      if (row) {
        body = (row.textContent || '').trim();
      }
    }
    if (!title && !body) {
      pop.hidden = true;
      return;
    }
    while (pop.firstChild) {
      pop.removeChild(pop.firstChild);
    }
    if (title) {
      const tDiv = document.createElement('div');
      tDiv.className = 'popover-title';
      tDiv.textContent = title;
      pop.appendChild(tDiv);
    }
    if (body) {
      const bDiv = document.createElement('div');
      bDiv.className = 'popover-body';
      bDiv.textContent = body;
      pop.appendChild(bDiv);
    }
    pop.hidden = false;
  }

  function hidePopover() {
    const pop = document.getElementById('fb:popover');
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
    const selection = window.getSelection ? window.getSelection() : null;
    if (!selection || !document.createRange) {
      return;
    }
    const range = document.createRange();
    range.selectNodeContents(code);
    selection.removeAllRanges();
    selection.addRange(range);
  }

  // Copy only writes, on the reader's click. A refusal leaves the text selected to copy by hand.
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
      const btn =
        ev.target && typeof ev.target.closest === 'function'
          ? ev.target.closest('[data-copy-chip]')
          : null;
      if (btn) {
        copyChip(btn);
      }
    });
  }

  function initPageReader() {
    const reviewBanners = document.querySelectorAll('[data-review-by]');
    if (reviewBanners.length > 0) {
      const today = localIsoDate();
      for (let i = 0; i < reviewBanners.length; i += 1) {
        const banner = reviewBanners[i];
        const reviewBy = banner.getAttribute('data-review-by') || '';
        const pageDue = Boolean(reviewBy && today > reviewBy);
        const rows = banner.querySelectorAll('[data-claim-review-by]');
        let dueRows = 0;
        for (let j = 0; j < rows.length; j += 1) {
          const claimBy = rows[j].getAttribute('data-claim-review-by') || '';
          if (claimBy && today > claimBy) {
            rows[j].hidden = false;
            dueRows += 1;
          }
        }
        const claimDue = dueRows > 0;
        const dueBlock = banner.querySelector('.due-claims');
        if (dueBlock) {
          dueBlock.hidden = !claimDue;
        }
        const dueCount = banner.querySelector('.due-count');
        if (dueCount) {
          dueCount.textContent = String(dueRows);
        }
        const pageLine = banner.querySelector('.body');
        if (pageLine) {
          pageLine.hidden = !pageDue;
        }
        if (pageDue || claimDue) {
          banner.hidden = false;
        }
      }
    }

    const wordingSeg = document.getElementById('fb:wordingSeg');
    if (wordingSeg) {
      let storedWording = null;
      try {
        storedWording = window.localStorage.getItem(WORDING_KEY);
      } catch (_err) {
        storedWording = null;
      }
      if (storedWording === 'plain' || storedWording === 'both') {
        applyWording(storedWording);
      }
      wordingSeg.addEventListener('click', function (ev) {
        const btn =
          ev.target && typeof ev.target.closest === 'function'
            ? ev.target.closest('[data-wording]')
            : null;
        if (!btn) {
          return;
        }
        const mode = btn.getAttribute('data-wording') === 'plain' ? 'plain' : 'both';
        applyWording(mode);
        try {
          window.localStorage.setItem(WORDING_KEY, mode);
        } catch (_err) {
          const msg =
            window.FB_STRINGS && window.FB_STRINGS.not_remembered
              ? window.FB_STRINGS.not_remembered
              : '';
          showToast(msg);
        }
      });
    }

    const copyBtn = document.getElementById('fb:copyLinkBtn');
    if (copyBtn) {
      copyBtn.addEventListener('click', function () {
        const text = window.location ? window.location.href : '';
        if (navigator.clipboard && typeof navigator.clipboard.writeText === 'function') {
          navigator.clipboard.writeText(text).then(
            function () {
              showToast('Link copied');
            },
            function () {
              showToast('Copy failed');
            }
          );
        } else {
          showToast('Copy failed');
        }
      });
    }

    const reader = document.getElementById('fb:reader');
    if (reader) {
      reader.addEventListener('click', function (ev) {
        const mark =
          ev.target && typeof ev.target.closest === 'function'
            ? ev.target.closest('[data-claim]')
            : null;
        if (!mark) {
          return;
        }
        const claimId = mark.getAttribute('data-claim');
        const card = claimId ? document.getElementById('ev-' + claimId) : null;
        if (card) {
          if (typeof card.scrollIntoView === 'function') {
            card.scrollIntoView({ behavior: 'smooth', block: 'nearest' });
          }
          card.classList.add('flash');
          window.setTimeout(function () {
            card.classList.remove('flash');
          }, 1400);
        }
      });

      reader.addEventListener('mouseover', function (ev) {
        const pv =
          ev.target && typeof ev.target.closest === 'function'
            ? ev.target.closest('[data-pv]')
            : null;
        if (pv) {
          showPopoverFor(pv);
        }
      });

      reader.addEventListener('mouseout', function (ev) {
        const pv =
          ev.target && typeof ev.target.closest === 'function'
            ? ev.target.closest('[data-pv]')
            : null;
        if (pv) {
          hidePopover();
        }
      });

      reader.addEventListener('focusin', function (ev) {
        const pv =
          ev.target && typeof ev.target.closest === 'function'
            ? ev.target.closest('[data-pv]')
            : null;
        if (pv) {
          showPopoverFor(pv);
        }
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
    const regTable = document.getElementById('fb:registerTable');
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

    const filterBar = document.getElementById('fb:claimsFilter');
    const filterSeg = document.getElementById('fb:claimsFilterSeg');
    const emptyLine = document.getElementById('fb:registerEmptyFilter');
    const pager = document.getElementById('fb:registerPager');
    const pageInfo = document.getElementById('fb:registerPageInfo');
    const prevBtn = pager ? pager.querySelector('[data-pg="prev"]') : null;
    const nextBtn = pager ? pager.querySelector('[data-pg="next"]') : null;

    const announce = function (text) {
      const sr = document.getElementById('fb:sr');
      if (sr) {
        sr.textContent = text;
      }
    };

    // Without scripts the table is complete; with them a register shows 50 rows at a time.
    const render = function () {
      const pages = Math.max(1, Math.ceil(matching.length / PAGE_SIZE));
      if (page >= pages) {
        page = pages - 1;
      }
      if (page < 0) {
        page = 0;
      }
      const first = page * PAGE_SIZE;
      const visible = new Set(matching.slice(first, first + PAGE_SIZE));
      for (let i = 0; i < ordered.length; i += 1) {
        const hide = !visible.has(ordered[i]);
        if (ordered[i].hidden !== hide) {
          ordered[i].hidden = hide;
        }
      }
      if (pager) {
        pager.hidden = matching.length <= PAGE_SIZE;
        if (pageInfo) {
          pageInfo.textContent =
            'Rows ' +
            (first + 1) +
            ' to ' +
            Math.min(first + PAGE_SIZE, matching.length) +
            ' of ' +
            matching.length;
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
        const buttons = filterSeg.querySelectorAll('[data-cf]');
        for (let i = 0; i < buttons.length; i += 1) {
          buttons[i].setAttribute(
            'aria-pressed',
            buttons[i].getAttribute('data-cf') === filterVal ? 'true' : 'false'
          );
        }
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

    const headers = thead.querySelectorAll('th[scope="col"]');
    for (let i = 0; i < headers.length; i += 1) {
      const th = headers[i];
      const colName = th.getAttribute('data-col');
      if (!colName || th.querySelector('[data-sort-col]')) {
        continue;
      }
      const btn = document.createElement('button');
      btn.type = 'button';
      btn.className = 'th-sort';
      btn.setAttribute('data-sort-col', colName);
      btn.appendChild(document.createTextNode(colName));
      const arrow = document.createElement('span');
      arrow.className = 'sort-arrow';
      arrow.setAttribute('aria-hidden', 'true');
      btn.appendChild(arrow);
      th.textContent = '';
      th.appendChild(btn);
    }

    thead.addEventListener('click', function (ev) {
      const btn =
        ev.target && typeof ev.target.closest === 'function'
          ? ev.target.closest('[data-sort-col]')
          : null;
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
      for (let i = 0; i < all.length; i += 1) {
        all[i].removeAttribute('aria-sort');
      }
      th.setAttribute('aria-sort', nextDir);

      const sign = nextDir === 'ascending' ? 1 : -1;
      let colIndex = 0;
      for (let i = 0; i < all.length; i += 1) {
        if (all[i] === th) {
          colIndex = i;
        }
      }
      const keys = new Map();
      for (let i = 0; i < ordered.length; i += 1) {
        const cell = ordered[i].cells[colIndex];
        keys.set(
          ordered[i],
          !cell
            ? ''
            : cell.hasAttribute('data-sort-val')
              ? cell.getAttribute('data-sort-val')
              : cell.textContent.trim()
        );
      }
      ordered.sort(function (a, b) {
        const byVal = collate(keys.get(a), keys.get(b));
        if (byVal !== 0) {
          return byVal * sign;
        }
        return collate(a.id || '', b.id || '') * sign;
      });
      for (let i = 0; i < ordered.length; i += 1) {
        tbody.appendChild(ordered[i]);
      }
      page = 0;
      applyFilter();
      render();
    });

    if (filterBar && filterSeg) {
      filterBar.hidden = false;
      filterSeg.addEventListener('click', function (ev) {
        const btn =
          ev.target && typeof ev.target.closest === 'function'
            ? ev.target.closest('[data-cf]')
            : null;
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
        const btn =
          ev.target && typeof ev.target.closest === 'function'
            ? ev.target.closest('[data-pg]')
            : null;
        if (!btn || btn.disabled) {
          return;
        }
        page += btn.getAttribute('data-pg') === 'next' ? 1 : -1;
        render();
        const wrap = regTable.parentNode;
        if (wrap && typeof wrap.scrollIntoView === 'function') {
          wrap.scrollIntoView({ block: 'start' });
        }
        announce(pageInfo ? pageInfo.textContent : '');
      });
    }

    const rowById = function (id) {
      for (let i = 0; i < ordered.length; i += 1) {
        if (ordered[i].id === id) {
          return ordered[i];
        }
      }
      return null;
    };

    // A link to a row (`claims.html#CL-...`) must land on that row, whatever page or filter hides it.
    const revealHash = function () {
      let id = '';
      try {
        id = decodeURIComponent(window.location.hash.replace(/^#/, ''));
      } catch (err) {
        id = '';
      }
      // Look the id up among the register's own rows: a frame element must never answer for a row.
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
      if (typeof row.scrollIntoView === 'function') {
        row.scrollIntoView({ block: 'center' });
      }
    };
    window.addEventListener('hashchange', revealHash);

    render();
    revealHash();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initFrame);
  } else {
    initFrame();
  }
})();
