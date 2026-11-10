// Narrow-width probe (card narrow-width-check). Injected into the framed page;
// measures the page and top bar at load and posts the measures to the parent.
(function () {
  'use strict';

  window.addEventListener('load', function () {
    window.setTimeout(function () {
      var result;
      try {
        var header = document.querySelector('header.topbar');
        var topbar = header ? {
          scrollWidth: header.scrollWidth,
          clientWidth: header.clientWidth
        } : null;
        var nodes = document.querySelectorAll('header.topbar button, header.topbar a');
        var controls = [];
        for (var i = 0; i < nodes.length; i++) {
          var el = nodes[i];
          if (!el.getClientRects().length) {
            continue;
          }
          var rect = el.getBoundingClientRect();
          var name = el.id || el.getAttribute('aria-label');
          if (!name) {
            var text = (el.textContent || '').trim();
            if (text) {
              name = text.length > 40 ? text.slice(0, 40) : text;
            }
          }
          if (!name) {
            name = el.getAttribute('class') || '';
          }
          controls.push({
            name: name,
            tag: el.tagName.toLowerCase(),
            icon: el.classList.contains('icon-btn'),
            // The one status link (a.build-date-link) is a linked chip, with a 24px design floor, left for its own card.
            chip: el.tagName === 'A' && el.classList.contains('build-date-link') && !!el.querySelector('.chip'),
            width: Math.round(rect.width * 100) / 100,
            height: Math.round(rect.height * 100) / 100
          });
        }
        result = {
          narrow: 1,
          innerWidth: window.innerWidth,
          clientWidth: document.documentElement.clientWidth,
          pageScrollWidth: document.documentElement.scrollWidth,
          topbar: topbar,
          controls: controls
        };
      } catch (e) {
        result = {
          narrow: 1,
          error: String(e)
        };
      }
      window.parent.postMessage(JSON.stringify(result), '*');
    }, 50);
  });
}());
