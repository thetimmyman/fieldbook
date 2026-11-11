// Narrow-width harness (card narrow-width-check). Runs in the parent page,
// frames the page under test at 375px width, receives measures from the probe,
// and checks sideways scrolling and control heights.
(function () {
  'use strict';

  var deadline = Date.now() + 20000;
  var finished = false;
  var measures = null;
  var settled = false;

  window.addEventListener('message', function (ev) {
    if (measures) {
      return;
    }
    if (typeof ev.data !== 'string') {
      return;
    }
    var frame = document.getElementById('narrow-frame');
    if (!frame || !frame.contentWindow || ev.source !== frame.contentWindow) {
      return;
    }
    var data;
    try {
      data = JSON.parse(ev.data);
    } catch (e) {
      return;
    }
    if (data && typeof data === 'object' && data.narrow === 1) {
      measures = data;
      if (document.readyState !== 'loading') {
        settle();
      }
    }
  });

  function hold() {
    if (finished || Date.now() > deadline) {
      return;
    }
    if (!document.body) {
      document.addEventListener('DOMContentLoaded', hold);
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

  hold();

  document.addEventListener('DOMContentLoaded', function () {
    if (measures) {
      settle();
    }
  });

  var poll = window.setInterval(function () {
    if ((document.readyState !== 'loading' && measures) || Date.now() >= deadline) {
      settle();
    }
  }, 50);

  function settle() {
    if (settled) {
      return;
    }
    settled = true;
    if (poll) {
      window.clearInterval(poll);
      poll = null;
    }

    var out = document.getElementById('harness-results');
    var summary = document.getElementById('harness-summary');
    var counts = { run: 0, failed: 0 };

    function report(id, problems, notes) {
      var li = document.createElement('li');
      counts.run += 1;
      if (problems.length) {
        counts.failed += 1;
      }
      li.setAttribute('data-case', id);
      li.setAttribute('data-ok', problems.length ? 'false' : 'true');
      li.textContent = id + ' (' + id.replace(/_/g, ' ') + ')' +
        (problems.length ? ': ' + problems.join('; ') : ': ok') +
        (notes && notes.length ? ' [' + notes.join('; ') + ']' : '');
      if (out) {
        out.appendChild(li);
      }
    }

    function guarded(id, fn) {
      var problems = [];
      var notes = [];
      try {
        fn(problems, notes);
      } catch (e) {
        problems.push('threw ' + e);
      }
      report(id, problems, notes);
    }

    guarded('page_does_not_scroll_sideways_at_375', function (problems) {
      if (!measures) {
        problems.push('no result from the frame: the probe did not report');
        return;
      }
      if (measures.error) {
        problems.push(measures.error);
        return;
      }
      if (measures.innerWidth !== 375) {
        problems.push('the frame is ' + measures.innerWidth + ' px wide, not 375');
      }
      if (measures.clientWidth !== 375) {
        problems.push('the page lays out at ' + measures.clientWidth + ' px, not 375 (a scrollbar or a margin takes width)');
      }
      if (measures.pageScrollWidth > measures.clientWidth) {
        problems.push('the page is ' + measures.pageScrollWidth + ' px wide in a 375px frame');
      }
    });

    guarded('top_bar_does_not_scroll_sideways_at_375', function (problems) {
      if (!measures) {
        problems.push('no result from the frame: the probe did not report');
        return;
      }
      if (measures.error) {
        problems.push(measures.error);
        return;
      }
      if (!measures.topbar) {
        problems.push('no top bar in the page');
        return;
      }
      if (measures.topbar.scrollWidth > measures.topbar.clientWidth) {
        problems.push('the top bar scrolls sideways: scrollWidth ' +
          measures.topbar.scrollWidth + ' > clientWidth ' + measures.topbar.clientWidth);
      }
    });

    guarded('top_bar_buttons_keep_the_control_height', function (problems) {
      if (!measures) {
        problems.push('no result from the frame: the probe did not report');
        return;
      }
      if (measures.error) {
        problems.push(measures.error);
        return;
      }
      if (!measures.controls || !measures.controls.length) {
        problems.push('no top-bar button or link was measured');
        return;
      }
      var measured = 0;
      for (var i = 0; i < measures.controls.length; i++) {
        var c = measures.controls[i];
        measured += 1;
        var tag = c.tag === 'a' ? 'a' : 'button';
        if (c.height < c.floor) {
          problems.push(tag + ' "' + c.name + '" is ' + c.height + 'px tall, under ' + c.floor + 'px');
        } else if (c.icon && c.width < 32) {
          problems.push('icon ' + tag + ' "' + c.name + '" is ' + c.width + 'px wide, under 32px');
        }
      }
      if (!measured) {
        problems.push('no top-bar button was measured against the control height');
      }
    });

    finished = true;
    if (summary) {
      summary.setAttribute('data-run', String(counts.run));
      summary.setAttribute('data-failed', String(counts.failed));
      summary.setAttribute('data-done', 'true');
      summary.textContent = counts.run + ' cases, ' + counts.failed + ' failed';
    }
  }
}());
