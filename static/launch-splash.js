/** Puzzle pieces snap in 3D, then reveal the full animated logo. */
(function () {
  var splash = document.getElementById('launch-splash');
  if (!splash) return;

  var wrap = splash.querySelector('.ij-launch-logo-wrap');
  var logo = splash.querySelector('.reaper-logo-anim');
  if (!logo) return;

  var mergeMs = 1550;
  var loopPreview = document.body.classList.contains('splash-preview-loop');

  function resetPieces() {
    splash.querySelectorAll('.splash-piece').forEach(function (el) {
      el.style.animation = 'none';
      void el.offsetWidth;
      el.style.animation = '';
    });
  }

  function merge() {
    logo.classList.add('is-merged', 'is-assembled');
    if (wrap) wrap.classList.add('is-assembled');
  }

  function split() {
    logo.classList.remove('is-merged', 'is-assembled');
    if (wrap) wrap.classList.remove('is-assembled');
    resetPieces();
  }

  setTimeout(merge, mergeMs);

  if (loopPreview) {
    setInterval(function () {
      split();
      setTimeout(merge, mergeMs);
    }, 7500);
  }
})();
