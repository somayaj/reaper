/** Launch splash — animated welcome logo while the IDE loads. */
(function () {
  // WebView2: never mount the animated harvest SVG (compositor black-band bug).
  if (window.__reaperSkipSplash
      || (document.documentElement
          && document.documentElement.classList.contains('ij-platform-windows'))) {
    var doomed = document.getElementById('launch-splash');
    if (doomed) doomed.remove();
    document.body && document.body.classList.add('reaper-ui-ready');
    document.documentElement.classList.add('reaper-ui-ready');
    window.waitForLaunchSplashHarvest = function () { return Promise.resolve(); };
    return;
  }

  var splash = document.getElementById('launch-splash');
  if (!splash) return;

  var wrap = splash.querySelector('.ij-launch-logo-wrap');
  if (!wrap) return;

  var makeLogo = window.ReaperLogo && window.ReaperLogo.reaperLogoHtml;
  if (typeof makeLogo !== 'function') return;

  wrap.innerHTML = makeLogo('welcome', { extraClass: 'ij-welcome-logo logo-mark' });

  function restartAnimations(root) {
    if (!root) return;
    root.querySelectorAll('*').forEach(function (el) {
      var name = getComputedStyle(el).animationName;
      if (!name || name === 'none') return;
      el.style.animation = 'none';
      void el.offsetWidth;
      el.style.removeProperty('animation');
    });
  }

  requestAnimationFrame(function () {
    restartAnimations(wrap.querySelector('.reaper-logo-anim'));
  });

  window.addEventListener('reaper-logo-svg-ready', function () {
    if (!makeLogo) return;
    wrap.innerHTML = makeLogo('welcome', { extraClass: 'ij-welcome-logo logo-mark' });
    requestAnimationFrame(function () {
      restartAnimations(wrap.querySelector('.reaper-logo-anim'));
    });
  }, { once: true });

  window.__reaperSplashTiming = { totalMs: 0 };

  window.waitForLaunchSplashHarvest = function () {
    return Promise.resolve();
  };
})();
