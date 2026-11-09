// Harness self-test (edge-harness, AD-08). Injected only into the harness copy of an
// edition, never into the edition itself. Synchronous: it records every Content Security
// Policy violation and, at load, writes them and the page's resource entries into one
// <pre id="fb-selftest"> as JSON, which the harness reads from the dumped DOM.
(function () {
  "use strict";
  var violations = [];
  document.addEventListener("securitypolicyviolation", function (event) {
    violations.push({
      directive: String(event.effectiveDirective || event.violatedDirective || ""),
      blocked: String(event.blockedURI || "")
    });
  });
  window.addEventListener("load", function () {
    var resources = performance.getEntriesByType("resource").map(function (entry) {
      return { name: String(entry.name), initiator: String(entry.initiatorType) };
    });
    var pre = document.createElement("pre");
    pre.id = "fb-selftest";
    pre.textContent = JSON.stringify({ violations: violations, resources: resources });
    (document.body || document.documentElement).appendChild(pre);
  });
})();
