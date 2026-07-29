/** Launch splash — animated welcome logo while the IDE loads. */
(function () {
  var splash = document.getElementById('launch-splash');
  if (!splash) return;

  var isWindows = !!(window.__reaperWindowsSplash
    || (document.documentElement
        && document.documentElement.classList.contains('ij-platform-windows')));

  // Legacy hard-skip (tests / explicit opt-out only).
  if (window.__reaperSkipSplash && !isWindows) {
    splash.remove();
    document.body && document.body.classList.add('reaper-ui-ready');
    document.documentElement.classList.add('reaper-ui-ready');
    window.waitForLaunchSplashHarvest = function () { return Promise.resolve(); };
    return;
  }

  var wrap = splash.querySelector('.ij-launch-logo-wrap');
  if (!wrap) return;

  var makeLogo = window.ReaperLogo && window.ReaperLogo.reaperLogoHtml;
  if (typeof makeLogo !== 'function') return;

  wrap.innerHTML = makeLogo('welcome', { extraClass: 'ij-welcome-logo logo-mark' });

  function restartAnimations(root) {
    if (!root || isWindows) return;
    root.querySelectorAll('*').forEach(function (el) {
      var name = getComputedStyle(el).animationName;
      if (!name || name === 'none') return;
      el.style.animation = 'none';
      void el.offsetWidth;
      el.style.removeProperty('animation');
    });
  }

  if (!isWindows) {
    requestAnimationFrame(function () {
      restartAnimations(wrap.querySelector('.reaper-logo-anim'));
    });
  }

  window.addEventListener('reaper-logo-svg-ready', function () {
    if (!makeLogo) return;
    wrap.innerHTML = makeLogo('welcome', { extraClass: 'ij-welcome-logo logo-mark' });
    if (!isWindows) {
      requestAnimationFrame(function () {
        restartAnimations(wrap.querySelector('.reaper-logo-anim'));
      });
    }
  }, { once: true });

  // Windows: hold static logo briefly so startup shows branding.
  window.__reaperSplashTiming = { totalMs: isWindows ? 1400 : 0 };

  window.waitForLaunchSplashHarvest = function () {
    var total = (window.__reaperSplashTiming && window.__reaperSplashTiming.totalMs) || 0;
    if (!total) return Promise.resolve();
    var started = window.__reaperSplashAt || Date.now();
    var left = Math.max(0, total - (Date.now() - started));
    return new Promise(function (resolve) {
      setTimeout(resolve, left);
    });
  };
})();
