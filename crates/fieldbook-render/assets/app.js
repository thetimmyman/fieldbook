(function () {
  'use strict';

  const THEME_KEY = 'fieldbook.theme';

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
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', initFrame);
  } else {
    initFrame();
  }
})();
