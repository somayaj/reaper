/**
 * Shared UI icons — 24×24 stroke glyphs, currentColor, IntelliJ weight.
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

    /** Git commit — branch node with check-in */
    gitCommit: icon(
      '<path d="M6 3v12"/>' +
        '<circle cx="6" cy="18" r="3"/>' +
        '<circle cx="18" cy="6" r="3"/>' +
        '<path d="M6 15l8.5-9"/>' +
        '<path d="M14 6h4M16 4v4"/>',
    ),

    /** Pull / sync — circular arrows */
    gitPull: icon(
      '<path d="M4 4v5h.582m15.356 2A8.001 8.001 0 004.582 9m0 0H9m11 11v-5h-.581m0 0a8.003 8.003 0 01-15.357-2m15.357 2H15"/>',
    ),

    /** Push to remote — upload arrow */
    gitPush: icon(
      '<path d="M12 16V5"/>' +
        '<path d="M8 9l4-4 4 4"/>' +
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
  };
})();
