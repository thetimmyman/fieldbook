(function () {
  'use strict';

  const THEME_KEY = 'fieldbook.theme';
  const WORDING_KEY = 'fieldbook.wording';

  const ROUTE_SECTIONS = {
    'index.html': { section: 'today', item: 'today' },
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
    const toast = document.getElementById('toast');
    const sr = document.getElementById('sr');
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
    const btn = document.getElementById('themeBtn');
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
    const picks = document.querySelectorAll('#secSwitch [data-secpick]');
    for (let i = 0; i < picks.length; i += 1) {
      const btn = picks[i];
      const active = btn.getAttribute('data-secpick') === secKey;
      btn.setAttribute('aria-pressed', active ? 'true' : 'false');
    }
  }

  function isOverlayOpen() {
    const sidebar = document.getElementById('sidebar');
    return Boolean(sidebar && sidebar.classList.contains('open'));
  }

  function openSidebar() {
    const sidebar = document.getElementById('sidebar');
    const scrim = document.getElementById('scrim');
    const menuBtn = document.getElementById('menuBtn');
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
    const sidebar = document.getElementById('sidebar');
    const scrim = document.getElementById('scrim');
    const menuBtn = document.getElementById('menuBtn');
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

    const themeBtn = document.getElementById('themeBtn');
    if (themeBtn) {
      themeBtn.addEventListener('click', toggleTheme);
    }

    const menuBtn = document.getElementById('menuBtn');
    if (menuBtn) {
      menuBtn.addEventListener('click', function () {
        if (isOverlayOpen()) {
          closeSidebar(true);
        } else {
          openSidebar();
        }
      });
    }

    const closeBtn = document.getElementById('sidebarCloseBtn');
    if (closeBtn) {
      closeBtn.addEventListener('click', function () {
        closeSidebar(true);
      });
    }

    const scrim = document.getElementById('scrim');
    if (scrim) {
      scrim.addEventListener('click', function () {
        closeSidebar(true);
      });
    }

    const secSwitch = document.getElementById('secSwitch');
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
    const reader = document.getElementById('reader') || document.body;
    const isPlain = mode === 'plain';
    if (reader) {
      reader.classList.toggle('hide-precise', isPlain);
    }
    const buttons = document.querySelectorAll('#wordingSeg [data-wording]');
    for (let i = 0; i < buttons.length; i += 1) {
      const btn = buttons[i];
      const active = btn.getAttribute('data-wording') === (isPlain ? 'plain' : 'both');
      btn.setAttribute('aria-pressed', active ? 'true' : 'false');
    }
  }

  function showPopoverFor(el) {
    const pop = document.getElementById('popover');
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
    const pop = document.getElementById('popover');
    if (pop) {
      pop.hidden = true;
    }
  }

  function initPageReader() {
    const reviewBanners = document.querySelectorAll('[data-review-by]');
    if (reviewBanners.length > 0) {
      const today = localIsoDate();
      for (let i = 0; i < reviewBanners.length; i += 1) {
        const banner = reviewBanners[i];
        const reviewBy = banner.getAttribute('data-review-by') || '';
        if (reviewBy && today > reviewBy) {
          banner.hidden = false;
        }
      }
    }

    const wordingSeg = document.getElementById('wordingSeg');
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

    const copyBtn = document.getElementById('copyLinkBtn');
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

    const reader = document.getElementById('reader');
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
    const regTable = document.getElementById('registerTable');
    if (regTable) {
      const thead = regTable.querySelector('thead');
      const tbody = regTable.querySelector('tbody');
      if (thead && tbody) {
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
          const currentSort = th.getAttribute('aria-sort');
          const nextDir = currentSort === 'ascending' ? 'descending' : 'ascending';
          const headers = thead.querySelectorAll('th[scope="col"]');
          for (let i = 0; i < headers.length; i += 1) {
            headers[i].removeAttribute('aria-sort');
          }
          th.setAttribute('aria-sort', nextDir);

          const rows = Array.prototype.slice.call(tbody.querySelectorAll('tr'));
          const sign = nextDir === 'ascending' ? 1 : -1;
          rows.sort(function (a, b) {
            const cellA = a.querySelector('td[data-col="' + colName + '"]');
            const cellB = b.querySelector('td[data-col="' + colName + '"]');
            const valA = cellA ? cellA.getAttribute('data-sort-val') || '' : '';
            const valB = cellB ? cellB.getAttribute('data-sort-val') || '' : '';
            if (valA !== valB) {
              return (valA < valB ? -1 : 1) * sign;
            }
            const idA = a.id || '';
            const idB = b.id || '';
            return (idA < idB ? -1 : idA > idB ? 1 : 0) * sign;
          });
          for (let i = 0; i < rows.length; i += 1) {
            tbody.appendChild(rows[i]);
          }
        });
      }
    }

    const filterSeg = document.getElementById('claimsFilterSeg');
    if (filterSeg && regTable) {
      filterSeg.addEventListener('click', function (ev) {
        const btn =
          ev.target && typeof ev.target.closest === 'function'
            ? ev.target.closest('[data-cf]')
            : null;
        if (!btn) {
          return;
        }
        const filterVal = btn.getAttribute('data-cf') || 'all';
        const buttons = filterSeg.querySelectorAll('[data-cf]');
        for (let i = 0; i < buttons.length; i += 1) {
          const b = buttons[i];
          b.setAttribute(
            'aria-pressed',
            b.getAttribute('data-cf') === filterVal ? 'true' : 'false'
          );
        }
        const rows = regTable.querySelectorAll('tbody tr');
        let shown = 0;
        for (let i = 0; i < rows.length; i += 1) {
          const row = rows[i];
          const ver = row.getAttribute('data-verification') || '';
          const match = filterVal === 'all' || ver === filterVal;
          row.hidden = !match;
          if (match) {
            shown += 1;
          }
        }
        const emptyPanel = document.getElementById('registerEmptyFilter');
        if (emptyPanel) {
          emptyPanel.hidden = shown > 0;
        }
      });
    }
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initFrame);
  } else {
    initFrame();
  }
})();
