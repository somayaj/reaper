/**
 * Shared UI icons — 24×24 stroke glyphs, currentColor, medium stroke weight.
 */
(function () {
  const base =
    'fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"';

  function icon(paths, viewBox = '0 0 24 24') {
    return `<svg viewBox="${viewBox}" ${base} aria-hidden="true">${paths}</svg>`;
  }

  window.ReaperIcons = {
    /** New repository — folder with plus */
    newRepo: icon(
      '<path d="M3 7v10a2 2 0 002 2h14a2 2 0 002-2V9a2 2 0 00-2-2h-6l-2-2H5a2 2 0 00-2 2z"/>' +
        '<path d="M12 10v6M9 13h6"/>',
    ),

    /** Clone from URL — remote copy into local folder */
    clone: icon(
      '<path d="M7 16H5a2 2 0 01-2-2V6a2 2 0 012-2h8a2 2 0 012 2v1"/>' +
        '<path d="M9 16h8a2 2 0 002-2V8a2 2 0 00-2-2h-1"/>' +
        '<path d="M12 11v5M9.5 13.5L12 16l2.5-2.5"/>',
    ),

    /** Cursor agent — assistant bubble with sparkle */
    agent: icon(
      '<path d="M18 4l.75 1.5L20.25 6l-1.5.75L18 8.25l-.75-1.5L15.75 6l1.5-.75z"/>' +
        '<path d="M6 9.5a6 6 0 0112 0v4.5c0 1-1 2.5-2.5 3.2V19a1 1 0 01-1 1h-5a1 1 0 01-1-1v-1.8C7 16.5 6 15 6 14V9.5z"/>' +
        '<circle cx="9.5" cy="12" r=".85" fill="currentColor" stroke="none"/>' +
        '<circle cx="12" cy="12" r=".85" fill="currentColor" stroke="none"/>' +
        '<circle cx="14.5" cy="12" r=".85" fill="currentColor" stroke="none"/>' +
        '<path d="M9 17.5h6"/>',
    ),

    /** Git commit — branch node */
    gitCommit: icon(
      '<circle cx="6" cy="6" r="2.75"/>' +
        '<circle cx="6" cy="18" r="2.75"/>' +
        '<path d="M6 8.75v8.5"/>' +
        '<path d="M8.75 6h8.25"/>' +
        '<circle cx="18" cy="6" r="2.75"/>',
    ),

    /** Pull / sync — circular arrows */
    gitPull: icon(
      '<path d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/>',
    ),

    /** Push to remote — upload arrow */
    gitPush: icon(
      '<path d="M12 15V5"/>' +
        '<path d="M8.5 8.5L12 5l3.5 3.5"/>' +
        '<path d="M5 19h14"/>',
    ),

    /** Switch branch — git branch graph */
    gitBranch: icon(
      '<path d="M6 3v12"/>' +
        '<circle cx="6" cy="18" r="3"/>' +
        '<circle cx="18" cy="6" r="3"/>' +
        '<path d="M6 15l8.5-9"/>' +
        '<path d="M18 9v6a3 3 0 01-3 3h-3"/>',
    ),

    /** Publish to GitHub — cloud upload */
    gitPublish: icon(
      '<path d="M7 16a4 4 0 01-.88-7.903A5 5 0 1115.9 14H16a4 4 0 010 8H7z"/>' +
        '<path d="M12 12V8m0 0l-2 2m2-2l2 2"/>',
    ),

    /** Repository details — info card */
    gitRepo: icon(
      '<path d="M4 7v10c0 2 1 3 3 3h10c2 0 3-1 3-3V7c0-2-1-3-3-3H7c-2 0-3 1-3 3z"/>' +
        '<path d="M9 9h6M9 13h6"/>',
    ),

    /** Search — magnifying glass */
    search: icon(
      '<circle cx="11" cy="11" r="6.5"/>' +
        '<path d="M16 16l4.5 4.5"/>',
    ),

    toastError: icon(
      '<circle cx="12" cy="12" r="9"/>' +
        '<path d="M15 9l-6 6M9 9l6 6"/>',
    ),

    toastWarning: icon(
      '<path d="M12 9v4M12 17h.01"/>' +
        '<path d="M10.29 3.86L1.82 18a2 2 0 001.71 3h16.94a2 2 0 001.71-3L13.71 3.86a2 2 0 00-3.42 0z"/>',
    ),

    toastSuccess: icon(
      '<circle cx="12" cy="12" r="9"/>' +
        '<path d="M8 12l2.5 2.5L16 9"/>',
    ),

    toastInfo: icon(
      '<circle cx="12" cy="12" r="9"/>' +
        '<path d="M12 11v5M12 8h.01"/>',
    ),
  };
})();
