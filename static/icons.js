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
  };
})();
