/** Launch splash — animated welcome logo while the IDE loads. */
(function () {
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
